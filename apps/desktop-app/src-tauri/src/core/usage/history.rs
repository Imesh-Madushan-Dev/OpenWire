//! Rolling usage history.
//!
//! Three timeline tiers plus per-day breakdowns. The tiers exist so a month of
//! history costs a few thousand rows instead of two million: seconds are kept
//! only long enough for the live view, and everything older is already rolled
//! up by the time it is asked for.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use serde::{Deserialize, Serialize};

/// 5 minutes at one sample per second.
pub const SECONDS_KEPT: usize = 300;
/// 24 hours at one sample per minute.
pub const MINUTES_KEPT: usize = 24 * 60;
/// 90 days at one sample per hour.
pub const HOURS_KEPT: usize = 24 * 90;
/// How many daily breakdowns to retain.
pub const DAYS_KEPT: usize = 90;

/// Per-day cap on distinct keys. Beyond this the tail is folded into `Other`,
/// which keeps a browsing session with thousands of hosts from growing the
/// store without bound.
const MAX_APPS_PER_DAY: usize = 60;
const MAX_HOSTS_PER_DAY: usize = 120;
/// The network monitor sees the whole device, which reaches far more distinct
/// hosts in a day than the proxy does, so its maps get a wider cap.
const MAX_MONITORED_APPS_PER_DAY: usize = 120;
const MAX_MONITORED_HOSTS_PER_DAY: usize = 250;
pub const OTHER_KEY: &str = "Other";

/// How the interface counters behind the stored totals were read. Version 0
/// summed every interface Windows lists, filter drivers included, which
/// counted each adapter several times over - Wi-Fi six times on a typical
/// machine. Totals from then cannot be corrected, only set aside.
pub const COUNTING: u8 = 1;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    /// Unix seconds at the start of the bucket.
    pub t: i64,
    /// Everything across this machine's network interfaces.
    pub up: u64,
    pub down: u64,
    /// The subset that went through the tunnel.
    pub tunnel_up: u64,
    pub tunnel_down: u64,
}

impl Sample {
    fn add(&mut self, other: &Sample) {
        self.up += other.up;
        self.down += other.down;
        self.tunnel_up += other.tunnel_up;
        self.tunnel_down += other.tunnel_down;
    }
}

/// A timeline for an explicit span of time, with the resolution it came at.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpanTimeline {
    /// Seconds each sample covers: 60 from the minute tier, 3600 from hours.
    pub bucket_secs: u32,
    pub samples: Vec<Sample>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pair {
    pub up: u64,
    pub down: u64,
}

impl Pair {
    pub fn total(&self) -> u64 {
        self.up + self.down
    }

    fn add(&mut self, up: u64, down: u64) {
        self.up += up;
        self.down += down;
    }
}

/// Ranked maps for one source of attribution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Attributed {
    pub up: u64,
    pub down: u64,
    pub apps: HashMap<String, Pair>,
    pub hosts: HashMap<String, Pair>,
    pub kinds: HashMap<String, Pair>,
}

impl Attributed {
    pub fn is_empty(&self) -> bool {
        self.up == 0 && self.down == 0 && self.apps.is_empty()
    }

    fn merge(&mut self, other: &Attributed) {
        self.up += other.up;
        self.down += other.down;
        merge_into(&mut self.apps, &other.apps);
        merge_into(&mut self.hosts, &other.hosts);
        merge_into(&mut self.kinds, &other.kinds);
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DayBreakdown {
    /// The interface counters' total while the app ran. Read [`Self::totals`]
    /// rather than these.
    pub up: u64,
    pub down: u64,
    /// The [`COUNTING`] that `up` and `down` were read with.
    pub counting: u8,
    pub tunnel_up: u64,
    pub tunnel_down: u64,
    /// What went through OpenWire's proxy, attributed by the relay.
    pub apps: HashMap<String, Pair>,
    pub hosts: HashMap<String, Pair>,
    pub kinds: HashMap<String, Pair>,
    /// Everything else on the device, attributed by the network monitor
    /// service. Empty on days the monitor was not running. Never overlaps the
    /// relay maps: the monitor leaves out OpenWire's own process, whose traffic
    /// the relay has already attributed to the apps behind it.
    #[serde(skip_serializing_if = "Attributed::is_empty")]
    pub monitored: Attributed,
    /// ISO country code per host, from the address it was reached at, for
    /// the flag beside it. One map for both sources: where a host is does not
    /// depend on who counted it. Only named hosts are kept - see
    /// [`History::record_country`] - so it is bounded by the host caps.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub countries: HashMap<String, String>,
}

impl DayBreakdown {
    /// The day's traffic, as fully as it is known.
    ///
    /// Two sources, each blind to something: the interface counters run only
    /// while the app does, and the monitor and relay see only traffic they
    /// can name an app for - but the monitor is a service, recording whether
    /// or not the app is open. Each is a floor on the truth, so the higher
    /// is the better answer. Counters read the old way are left out: they
    /// overstate by several times, which no floor can fix.
    pub fn totals(&self) -> (u64, u64) {
        let attributed = (
            self.monitored.up + self.tunnel_up,
            self.monitored.down + self.tunnel_down,
        );
        let counted = if self.counting == COUNTING {
            (self.up, self.down)
        } else {
            (0, 0)
        };
        (counted.0.max(attributed.0), counted.1.max(attributed.1))
    }

    /// Folds the smallest entries into `Other` once a map outgrows its cap.
    fn compact(map: &mut HashMap<String, Pair>, cap: usize) {
        if map.len() <= cap {
            return;
        }

        let mut entries: Vec<(String, Pair)> = map.drain().collect();
        // Largest first, so the tail we fold away is genuinely the small stuff.
        entries.sort_unstable_by(|a, b| b.1.total().cmp(&a.1.total()).then_with(|| a.0.cmp(&b.0)));

        let mut other = Pair::default();
        for (_, pair) in entries.split_off(cap.saturating_sub(1)) {
            other.add(pair.up, pair.down);
        }

        map.extend(entries);
        if other.total() > 0 {
            map.entry(OTHER_KEY.to_string())
                .or_default()
                .add(other.up, other.down);
        }
    }
}

/// Which tier a query reads from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Range {
    FiveMinutes,
    ThreeHours,
    Day,
    Week,
    Month,
}

impl Range {
    /// Number of whole local days a breakdown query covers.
    pub fn days(self) -> usize {
        match self {
            Self::FiveMinutes | Self::ThreeHours | Self::Day => 1,
            Self::Week => 7,
            Self::Month => 30,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct History {
    /// Live tail; never persisted, since it is stale the moment the app stops.
    #[serde(skip)]
    seconds: VecDeque<Sample>,
    minutes: VecDeque<Sample>,
    hours: VecDeque<Sample>,
    /// Keyed by local day number (days since the Unix epoch in local time).
    ///
    /// Each day is stored in a file of its own rather than inline here: with
    /// device-wide monitoring a quarter of breakdowns runs to megabytes, and
    /// rewriting all of it every flush to change only today is wasted disk.
    #[serde(skip)]
    days: BTreeMap<i64, DayBreakdown>,
    /// Stores written before days had files of their own kept them inline.
    /// Read once, moved into `days`, and never written back.
    #[serde(rename = "days", skip_serializing)]
    legacy_days: BTreeMap<i64, DayBreakdown>,
    /// Days changed since the last flush.
    #[serde(skip)]
    dirty: BTreeSet<i64>,

    /// Executable path per app name, so an icon can still be shown for an app
    /// that only appears in history from a previous run.
    app_paths: HashMap<String, String>,

    /// The buckets still being filled. Persisted: skipping them threw away up
    /// to a minute of the 24-hour tier and up to an hour of the week/month
    /// tiers on every restart, which made those graphs look empty after a
    /// relaunch.
    minute_acc: Option<Sample>,
    hour_acc: Option<Sample>,

    /// The [`COUNTING`] the timeline tiers were recorded with.
    counting: u8,
}

impl History {
    /// Drops timeline samples counted the old way. Called once after loading:
    /// a graph several times too tall is worse than a short one.
    pub fn retire_old_counting(&mut self) {
        if self.counting != COUNTING {
            self.minutes.clear();
            self.hours.clear();
            self.minute_acc = None;
            self.hour_acc = None;
            self.counting = COUNTING;
        }
    }

    /// Records one second of throughput.
    pub fn push(&mut self, sample: Sample) {
        push_capped(&mut self.seconds, sample, SECONDS_KEPT);
        roll(
            &mut self.minute_acc,
            &mut self.minutes,
            sample,
            60,
            MINUTES_KEPT,
        );
        roll(
            &mut self.hour_acc,
            &mut self.hours,
            sample,
            3600,
            HOURS_KEPT,
        );
    }

    /// Attributes a slice of tunnel traffic to a day bucket.
    pub fn record_breakdown(
        &mut self,
        local_day: i64,
        app: &str,
        host: &str,
        kind: &str,
        up: u64,
        down: u64,
    ) {
        self.dirty.insert(local_day);
        let day = self.days.entry(local_day).or_default();
        day.tunnel_up += up;
        day.tunnel_down += down;
        day.apps.entry(app.to_string()).or_default().add(up, down);
        day.hosts.entry(host.to_string()).or_default().add(up, down);
        day.kinds.entry(kind.to_string()).or_default().add(up, down);

        DayBreakdown::compact(&mut day.apps, MAX_APPS_PER_DAY);
        DayBreakdown::compact(&mut day.hosts, MAX_HOSTS_PER_DAY);
    }

    /// Attributes traffic the network monitor saw outside OpenWire to a day.
    pub fn record_monitored(
        &mut self,
        local_day: i64,
        app: &str,
        host: &str,
        kind: &str,
        up: u64,
        down: u64,
    ) {
        self.dirty.insert(local_day);
        let monitored = &mut self.days.entry(local_day).or_default().monitored;
        monitored.up += up;
        monitored.down += down;
        monitored
            .apps
            .entry(app.to_string())
            .or_default()
            .add(up, down);
        monitored
            .hosts
            .entry(host.to_string())
            .or_default()
            .add(up, down);
        monitored
            .kinds
            .entry(kind.to_string())
            .or_default()
            .add(up, down);

        DayBreakdown::compact(&mut monitored.apps, MAX_MONITORED_APPS_PER_DAY);
        DayBreakdown::compact(&mut monitored.hosts, MAX_MONITORED_HOSTS_PER_DAY);
    }

    /// Remembers which country a host was reached in. Call after recording
    /// the host's bytes: a host the day no longer lists by name - folded into
    /// `Other` - is not kept, which bounds the map by the host caps however
    /// many hosts the day passes through.
    pub fn record_country(&mut self, local_day: i64, host: &str, country: &str) {
        let Some(day) = self.days.get_mut(&local_day) else {
            return;
        };
        let DayBreakdown {
            hosts,
            monitored,
            countries,
            ..
        } = day;
        let listed = |host: &str| hosts.contains_key(host) || monitored.hosts.contains_key(host);

        // Checked first so the steady state - the same host each second -
        // costs one lookup and no allocation.
        if countries.get(host).is_some_and(|known| known == country)
            || host == OTHER_KEY
            || !listed(host)
        {
            return;
        }

        if countries.len() >= MAX_HOSTS_PER_DAY + MAX_MONITORED_HOSTS_PER_DAY {
            countries.retain(|host, _| listed(host));
        }
        countries.insert(host.to_string(), country.to_string());
        self.dirty.insert(local_day);
    }

    /// Records system-wide totals against a day, whether or not they were ours.
    pub fn record_day_totals(&mut self, local_day: i64, up: u64, down: u64) {
        self.dirty.insert(local_day);
        let day = self.days.entry(local_day).or_default();
        if day.counting != COUNTING {
            // Counted the old way: overstated, and inseparable from what
            // follows, so the day's count starts over.
            day.up = 0;
            day.down = 0;
            day.counting = COUNTING;
        }
        day.up += up;
        day.down += down;
    }

    /// Installs days read from their own files, then folds in any a pre-split
    /// store still carried inline. Those are marked dirty so the next flush
    /// gives them files too; a day present in both keeps its file's copy,
    /// which is the newer one.
    pub fn adopt_days(&mut self, loaded: BTreeMap<i64, DayBreakdown>) {
        self.days.extend(loaded);
        for (day, breakdown) in std::mem::take(&mut self.legacy_days) {
            if let std::collections::btree_map::Entry::Vacant(slot) = self.days.entry(day) {
                slot.insert(breakdown);
                self.dirty.insert(day);
            }
        }
    }

    /// Copies of every day changed since the last call, for writing out. Days
    /// already pruned are skipped - their files are deleted, not rewritten.
    pub fn take_dirty(&mut self) -> Vec<(i64, DayBreakdown)> {
        std::mem::take(&mut self.dirty)
            .into_iter()
            .filter_map(|day| {
                self.days
                    .get(&day)
                    .map(|breakdown| (day, breakdown.clone()))
            })
            .collect()
    }

    /// Puts days back on the dirty list after a failed write, so they are
    /// retried on the next flush instead of silently going stale on disk.
    pub fn mark_dirty(&mut self, days: impl IntoIterator<Item = i64>) {
        self.dirty.extend(days);
    }

    /// The first day still inside the retention window.
    pub fn retention_cutoff(today: i64) -> i64 {
        today - DAYS_KEPT as i64 + 1
    }

    pub fn app_paths(&self) -> &HashMap<String, String> {
        &self.app_paths
    }

    pub fn set_app_paths(&mut self, paths: HashMap<String, String>) {
        self.app_paths = paths;
    }

    /// Drops anything past the retention window.
    pub fn prune(&mut self, today: i64) {
        let cutoff = Self::retention_cutoff(today);
        self.days.retain(|day, _| *day >= cutoff);
        trim(&mut self.minutes, MINUTES_KEPT);
        trim(&mut self.hours, HOURS_KEPT);

        // Paths are only worth keeping for apps a retained day still names.
        let days = &self.days;
        self.app_paths.retain(|app, _| {
            days.values()
                .any(|day| day.apps.contains_key(app) || day.monitored.apps.contains_key(app))
        });
    }

    /// Timeline for a range, including the bucket still being filled so the
    /// newest point is never missing.
    pub fn timeline(&self, range: Range) -> Vec<Sample> {
        let (series, in_progress, wanted) = match range {
            Range::FiveMinutes => (&self.seconds, None, SECONDS_KEPT),
            Range::ThreeHours => (&self.minutes, self.minute_acc, 180),
            Range::Day => (&self.minutes, self.minute_acc, MINUTES_KEPT),
            Range::Week => (&self.hours, self.hour_acc, 24 * 7),
            Range::Month => (&self.hours, self.hour_acc, 24 * 30),
        };

        let skip = series.len().saturating_sub(wanted);
        let mut out: Vec<Sample> = series.iter().skip(skip).copied().collect();
        if let Some(current) = in_progress {
            out.push(current);
        }
        out
    }

    /// Samples whose buckets overlap `[from, to)`, read from the minute tier
    /// while it still reaches back to `from` and from the hour tier otherwise
    /// - so today draws minute by minute, and any older span hour by hour.
    /// Like [`Self::timeline`], the bucket still being filled is included, so
    /// the newest point is never missing.
    ///
    /// Overlap rather than start, because hour buckets are cut on UTC hours:
    /// in a half-hour timezone like +5:30, a local day's first hour sits in a
    /// bucket that began the evening before.
    pub fn timeline_between(&self, from: i64, to: i64) -> SpanTimeline {
        let minutes_reach = self
            .minutes
            .front()
            .or(self.minute_acc.as_ref())
            .map(|s| s.t);
        let (series, in_progress, bucket_secs) = match minutes_reach {
            Some(first) if first <= from => (&self.minutes, self.minute_acc, 60_u32),
            _ => (&self.hours, self.hour_acc, 3600_u32),
        };
        let span = i64::from(bucket_secs);

        let samples = if to <= from {
            Vec::new()
        } else {
            series
                .iter()
                .copied()
                .chain(in_progress)
                .filter(|sample| sample.t + span > from && sample.t < to)
                .collect()
        };

        SpanTimeline {
            bucket_secs,
            samples,
        }
    }

    /// Merged breakdown across the last `range.days()` local days.
    pub fn summary(&self, range: Range, today: i64) -> DayBreakdown {
        self.summary_between(today - range.days() as i64 + 1, today)
    }

    /// Merged breakdown across an arbitrary inclusive span of local days.
    ///
    /// Days outside the retention window simply contribute nothing, so a range
    /// reaching further back than the store goes is clamped by omission rather
    /// than rejected.
    pub fn summary_between(&self, first: i64, last: i64) -> DayBreakdown {
        let mut merged = DayBreakdown {
            counting: COUNTING,
            ..DayBreakdown::default()
        };
        if last < first {
            return merged;
        }

        for (_, day) in self.days.range(first..=last) {
            let (up, down) = day.totals();
            merged.up += up;
            merged.down += down;
            merged.tunnel_up += day.tunnel_up;
            merged.tunnel_down += day.tunnel_down;
            merge_into(&mut merged.apps, &day.apps);
            merge_into(&mut merged.hosts, &day.hosts);
            merge_into(&mut merged.kinds, &day.kinds);
            merged.monitored.merge(&day.monitored);
            // Later days win: a host that moved is shown where it is now.
            merged
                .countries
                .extend(day.countries.iter().map(|(host, code)| (host.clone(), code.clone())));
        }

        merged
    }

    /// First day in a span the network monitor recorded anything, so the UI
    /// can say where device-wide figures begin.
    pub fn first_monitored_day(&self, first: i64, last: i64) -> Option<i64> {
        if last < first {
            return None;
        }
        self.days
            .range(first..=last)
            .find(|(_, day)| !day.monitored.is_empty())
            .map(|(day, _)| *day)
    }

    /// First and last local day that hold any record, for building a picker
    /// that only offers periods with data behind them.
    pub fn recorded_span(&self) -> Option<(i64, i64)> {
        let first = *self.days.keys().next()?;
        let last = *self.days.keys().next_back()?;
        Some((first, last))
    }
}

fn merge_into(target: &mut HashMap<String, Pair>, source: &HashMap<String, Pair>) {
    for (key, pair) in source {
        target
            .entry(key.clone())
            .or_default()
            .add(pair.up, pair.down);
    }
}

fn push_capped(series: &mut VecDeque<Sample>, sample: Sample, keep: usize) {
    series.push_back(sample);
    trim(series, keep);
}

fn trim(series: &mut VecDeque<Sample>, keep: usize) {
    while series.len() > keep {
        series.pop_front();
    }
}

/// Accumulates into the current bucket, emitting the previous one when the
/// clock crosses a boundary.
fn roll(
    acc: &mut Option<Sample>,
    out: &mut VecDeque<Sample>,
    sample: Sample,
    span: i64,
    keep: usize,
) {
    let slot = sample.t - sample.t.rem_euclid(span);

    match acc {
        Some(current) if current.t == slot => current.add(&sample),
        Some(current) => {
            let finished = *current;
            push_capped(out, finished, keep);
            *acc = Some(Sample { t: slot, ..sample });
        }
        None => *acc = Some(Sample { t: slot, ..sample }),
    }
}

/// Ranked view of one breakdown map, largest first, with `Other` forced last.
pub fn rank(map: &HashMap<String, Pair>, limit: usize) -> Vec<(String, Pair)> {
    let mut entries: Vec<(String, Pair)> = map.iter().map(|(k, v)| (k.clone(), *v)).collect();

    entries.sort_unstable_by(|a, b| {
        let a_other = a.0 == OTHER_KEY;
        let b_other = b.0 == OTHER_KEY;
        a_other
            .cmp(&b_other)
            .then_with(|| b.1.total().cmp(&a.1.total()))
            .then_with(|| a.0.cmp(&b.0))
    });

    entries.truncate(limit);
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(t: i64, down: u64) -> Sample {
        Sample {
            t,
            up: down / 4,
            down,
            tunnel_up: down / 8,
            tunnel_down: down / 2,
        }
    }

    #[test]
    fn seconds_roll_up_into_minutes_and_hours() {
        let mut history = History::default();

        // Two full minutes of one sample per second.
        for second in 0..120 {
            history.push(sample(second, 1_000));
        }

        // The first minute has closed; the second is still accumulating.
        let minutes = history.timeline(Range::Day);
        assert_eq!(minutes.len(), 2);
        assert_eq!(minutes[0].t, 0);
        assert_eq!(minutes[0].down, 60_000, "a closed minute sums its seconds");
        assert_eq!(minutes[1].t, 60);

        // Nothing has crossed an hour boundary, so only the open bucket exists.
        let hours = history.timeline(Range::Week);
        assert_eq!(hours.len(), 1);
        assert_eq!(hours[0].down, 120_000);
    }

    #[test]
    fn timeline_between_reads_minutes_while_they_reach_back() {
        let mut history = History::default();
        // Two hours from t = 0: the minute tier reaches all the way back.
        for second in 0..7_200 {
            history.push(sample(second, 10));
        }

        let first_hour = history.timeline_between(0, 3_600);
        assert_eq!(first_hour.bucket_secs, 60);
        assert_eq!(
            first_hour.samples.len(),
            60,
            "one sample per minute of the hour"
        );
        assert!(first_hour.samples.iter().all(|s| (0..3_600).contains(&s.t)));
        assert_eq!(first_hour.samples[0].down, 600, "a minute sums its seconds");

        // The open minute is part of the newest span.
        let tail = history.timeline_between(7_140, 7_200);
        assert_eq!(tail.samples.len(), 1);
        assert_eq!(tail.samples[0].t, 7_140);
    }

    #[test]
    fn timeline_between_falls_back_to_hours_for_older_spans() {
        let mut history = History::default();
        for second in 0..7_200 {
            history.push(sample(second, 10));
        }

        // Starts before the first minute bucket, so only hours can cover it.
        let span = history.timeline_between(-3_600, 7_200);
        assert_eq!(span.bucket_secs, 3_600);
        let starts: Vec<i64> = span.samples.iter().map(|s| s.t).collect();
        assert_eq!(starts, vec![0, 3_600], "the closed hour, then the open one");
        assert_eq!(span.samples[0].down, 36_000);
    }

    #[test]
    fn timeline_between_keeps_a_bucket_straddling_the_start() {
        let mut history = History::default();
        for second in 0..10_800 {
            history.push(sample(second, 10));
        }

        // A day starting on the half hour, as local days do at +5:30: the
        // bucket from 0 to 3600 covers its first half hour, so it belongs.
        let span = history.timeline_between(-1_800, 1_800);
        assert_eq!(span.bucket_secs, 3_600);
        let starts: Vec<i64> = span.samples.iter().map(|s| s.t).collect();
        assert_eq!(starts, vec![0]);
    }

    #[test]
    fn timeline_between_an_empty_span_is_empty() {
        let mut history = History::default();
        for second in 0..120 {
            history.push(sample(second, 10));
        }
        assert!(history.timeline_between(60, 60).samples.is_empty());
        assert!(history.timeline_between(90, 30).samples.is_empty());
        assert!(History::default()
            .timeline_between(0, 3_600)
            .samples
            .is_empty());
    }

    #[test]
    fn seconds_window_is_bounded() {
        let mut history = History::default();
        for second in 0..(SECONDS_KEPT as i64 + 250) {
            history.push(sample(second, 10));
        }
        assert_eq!(history.timeline(Range::FiveMinutes).len(), SECONDS_KEPT);
    }

    #[test]
    fn summary_merges_only_days_inside_the_range() {
        let mut history = History::default();
        let today = 20_000;

        history.record_breakdown(today, "edge.exe", "a.example", "HTTPS", 10, 90);
        history.record_breakdown(today - 3, "edge.exe", "b.example", "HTTPS", 5, 45);
        // Inside the 30-day window but outside the 7-day one.
        history.record_breakdown(today - 20, "old.exe", "c.example", "HTTP", 1, 1);
        // Outside every window: a 30-day range covers today-29..=today.
        history.record_breakdown(today - 30, "ancient.exe", "d.example", "HTTP", 1, 1);

        let day = history.summary(Range::Day, today);
        assert_eq!(day.tunnel_down, 90);
        assert_eq!(day.hosts.len(), 1);

        let week = history.summary(Range::Week, today);
        assert_eq!(week.tunnel_down, 135);
        assert_eq!(week.apps.get("edge.exe").unwrap().total(), 150);
        assert_eq!(week.hosts.len(), 2, "week spans both recent days");

        let month = history.summary(Range::Month, today);
        assert_eq!(month.hosts.len(), 3, "30 days reaches back 29 days, not 30");
    }

    #[test]
    fn pruning_drops_days_past_retention() {
        let mut history = History::default();
        let today = 20_000;
        history.record_breakdown(today, "a", "a", "HTTPS", 1, 1);
        history.record_breakdown(today - 200, "b", "b", "HTTPS", 1, 1);

        history.prune(today);
        assert_eq!(history.summary(Range::Month, today).hosts.len(), 1);
        // The pruned day is gone entirely, not merely excluded from the range.
        assert!(history.days.len() == 1);
    }

    #[test]
    fn oversized_maps_fold_their_tail_into_other() {
        let mut history = History::default();
        let today = 1;

        // Big entries first so the small ones are what gets folded away.
        for index in 0..(MAX_HOSTS_PER_DAY + 40) {
            let weight = (MAX_HOSTS_PER_DAY + 40 - index) as u64;
            history.record_breakdown(today, "app", &format!("host{index}"), "HTTPS", 0, weight);
        }

        let day = history.summary(Range::Day, today);
        assert!(day.hosts.len() <= MAX_HOSTS_PER_DAY, "cap enforced");
        assert!(day.hosts.contains_key(OTHER_KEY), "tail folded into Other");
        // Folding must not lose bytes.
        let counted: u64 = day.hosts.values().map(Pair::total).sum();
        assert_eq!(counted, day.tunnel_down);
        assert!(day.hosts.contains_key("host0"), "largest entry survives");
    }

    /// The regression this guards: the open minute and hour buckets used to be
    /// `#[serde(skip)]`, so every relaunch silently dropped up to an hour of
    /// the rolled-up tiers and the week graph came back near-empty.
    #[test]
    fn an_open_bucket_survives_a_save_and_reload() {
        let mut history = History::default();
        for second in 0..90 {
            history.push(sample(second, 1_000));
        }
        history.record_breakdown(7, "edge.exe", "a.example", "HTTPS", 1, 9);
        history.set_app_paths(HashMap::from([(
            "edge.exe".to_string(),
            r"C:\edge.exe".to_string(),
        )]));

        let before = history.timeline(Range::Week);
        let json = serde_json::to_string(&history).expect("serialize");
        let days: BTreeMap<i64, DayBreakdown> = history.take_dirty().into_iter().collect();
        let mut after: History = serde_json::from_str(&json).expect("deserialize");
        after.adopt_days(days);

        assert_eq!(after.timeline(Range::Week), before, "open hour bucket lost");
        assert_eq!(after.timeline(Range::Day), history.timeline(Range::Day));
        assert_eq!(after.summary(Range::Day, 7).apps.len(), 1);
        assert_eq!(
            after.app_paths().get("edge.exe").map(String::as_str),
            Some(r"C:\edge.exe"),
            "icon paths must survive a restart too"
        );
    }

    #[test]
    fn pruning_forgets_paths_for_apps_no_retained_day_names() {
        let mut history = History::default();
        let today = 20_000;
        history.record_breakdown(today, "kept.exe", "h", "HTTPS", 1, 1);
        history.record_breakdown(today - 200, "gone.exe", "h", "HTTPS", 1, 1);
        history.set_app_paths(HashMap::from([
            ("kept.exe".to_string(), r"C:\kept.exe".to_string()),
            ("gone.exe".to_string(), r"C:\gone.exe".to_string()),
        ]));

        history.prune(today);
        assert!(history.app_paths().contains_key("kept.exe"));
        assert!(!history.app_paths().contains_key("gone.exe"));
    }

    #[test]
    fn a_day_totals_the_fuller_of_its_two_sources() {
        let mut history = History::default();
        // The app ran briefly; the monitor service recorded all day.
        history.record_day_totals(4, 10, 100);
        history.record_monitored(4, "edge.exe", "a.example", "HTTPS", 30, 2_000);
        // And the interface counters saw upload the monitor could not name.
        history.record_day_totals(4, 50, 0);

        let day = history.summary_between(4, 4);
        assert_eq!((day.up, day.down), (60, 2_000));
    }

    #[test]
    fn totals_counted_the_old_way_are_set_aside() {
        // A day stored before filter interfaces were skipped: six times over.
        let stored: DayBreakdown = serde_json::from_str(
            r#"{"up":600,"down":6000,"monitored":{"up":100,"down":1000}}"#,
        )
        .expect("an old day file");
        assert_eq!(stored.counting, 0);
        assert_eq!(stored.totals(), (100, 1000), "the monitor's figures stand in");

        let mut history = History::default();
        history.adopt_days(BTreeMap::from([(9, stored)]));
        history.record_day_totals(9, 5, 7);
        let day = history.summary_between(9, 9);
        assert_eq!(
            (day.up, day.down),
            (100, 1000),
            "new counting starts the day over rather than adding to the old"
        );
    }

    #[test]
    fn timeline_samples_counted_the_old_way_are_dropped_once() {
        let mut history: History = serde_json::from_str(
            r#"{"minutes":[{"t":60,"up":6,"down":60,"tunnelUp":0,"tunnelDown":0}]}"#,
        )
        .expect("an old store");
        history.retire_old_counting();
        assert!(history.timeline(Range::Day).iter().all(|s| s.down == 0));

        history.push(Sample { t: 120, down: 5, ..Sample::default() });
        let json = serde_json::to_string(&history).expect("serialize");
        let mut reloaded: History = serde_json::from_str(&json).expect("reload");
        reloaded.retire_old_counting();
        assert_eq!(reloaded.counting, COUNTING, "and never again once current");
    }

    #[test]
    fn days_are_not_written_inline_any_more() {
        let mut history = History::default();
        history.record_breakdown(7, "edge.exe", "a.example", "HTTPS", 1, 9);

        let json = serde_json::to_string(&history).expect("serialize");
        assert!(
            !json.contains("a.example"),
            "days belong in their own files"
        );
        assert_eq!(
            history.take_dirty().len(),
            1,
            "the changed day is due a write"
        );
        assert!(history.take_dirty().is_empty(), "and only once");
    }

    #[test]
    fn a_store_with_inline_days_migrates_without_losing_them() {
        // The shape every store had before days moved into their own files.
        let legacy = r#"{"days":{"7":{"up":10,"down":90,"tunnelUp":1,"tunnelDown":9,
            "apps":{"edge.exe":{"up":1,"down":9}},"hosts":{},"kinds":{}}}}"#;
        let mut history: History = serde_json::from_str(legacy).expect("legacy store");

        // A file already written for the same day is newer and must win.
        let newer = DayBreakdown {
            tunnel_down: 500,
            ..DayBreakdown::default()
        };
        history.adopt_days(BTreeMap::from([(8, newer)]));

        // Read through what the relay attributed: the interface totals of a
        // store this old were counted the overstated way, and are set aside.
        let migrated = history.summary_between(7, 7);
        assert_eq!(migrated.tunnel_down, 9);
        assert_eq!(migrated.apps["edge.exe"].down, 9);
        assert_eq!(history.summary_between(8, 8).tunnel_down, 500);
        let dirty: Vec<i64> = history
            .take_dirty()
            .into_iter()
            .map(|(day, _)| day)
            .collect();
        assert_eq!(dirty, vec![7], "only the migrated day needs a file");
    }

    #[test]
    fn monitored_traffic_merges_separately_from_the_relay() {
        let mut history = History::default();
        history.record_breakdown(5, "edge.exe", "a.example", "HTTPS", 1, 9);
        history.record_monitored(5, "steam.exe", "cdn.example", "HTTPS", 0, 1_000);
        history.record_monitored(6, "steam.exe", "cdn.example", "HTTPS", 0, 500);

        let span = history.summary_between(5, 6);
        assert_eq!(span.tunnel_down, 9, "relay totals are untouched");
        assert_eq!(span.monitored.down, 1_500);
        assert_eq!(span.monitored.apps["steam.exe"].down, 1_500);
        assert!(!span.apps.contains_key("steam.exe"), "sources stay apart");
        assert_eq!(history.first_monitored_day(1, 9), Some(5));
        assert_eq!(history.first_monitored_day(7, 9), None);
    }

    #[test]
    fn a_day_without_monitoring_stays_byte_for_byte_compact() {
        let day = DayBreakdown {
            down: 1,
            ..DayBreakdown::default()
        };
        let json = serde_json::to_string(&day).expect("serialize");
        assert!(
            !json.contains("monitored"),
            "empty monitor maps cost nothing on disk"
        );
    }

    #[test]
    fn ranking_puts_other_last_regardless_of_size() {
        let mut map = HashMap::new();
        map.insert("small".to_string(), Pair { up: 0, down: 1 });
        map.insert(OTHER_KEY.to_string(), Pair { up: 0, down: 999 });
        map.insert("big".to_string(), Pair { up: 0, down: 500 });

        let ranked = rank(&map, 10);
        assert_eq!(ranked[0].0, "big");
        assert_eq!(ranked[1].0, "small");
        assert_eq!(ranked[2].0, OTHER_KEY);
    }

    #[test]
    fn countries_are_kept_only_for_hosts_the_day_still_names() {
        let mut history = History::default();
        history.record_breakdown(5, "edge.exe", "a.example", "HTTPS", 1, 9);
        history.record_country(5, "a.example", "SG");
        // Never recorded, or folded away: nothing to put a flag beside.
        history.record_country(5, "unseen.example", "US");
        history.record_country(5, OTHER_KEY, "US");
        history.record_country(9, "a.example", "US");

        let day = history.summary_between(5, 5);
        assert_eq!(day.countries.len(), 1);
        assert_eq!(day.countries["a.example"], "SG");

        history.record_monitored(6, "steam.exe", "a.example", "HTTPS", 0, 5);
        history.record_country(6, "a.example", "JP");
        assert_eq!(
            history.summary_between(5, 6).countries["a.example"],
            "JP",
            "the newest day says where the host is now"
        );
    }
}

#[cfg(test)]
mod range_tests {
    use super::*;

    fn history_with_days(days: &[i64]) -> History {
        let mut history = History::default();
        for (index, day) in days.iter().enumerate() {
            history.record_breakdown(*day, "app", &format!("h{index}"), "HTTPS", 1, 9);
        }
        history
    }

    #[test]
    fn a_custom_span_covers_both_ends_inclusively() {
        let history = history_with_days(&[10, 11, 12, 13]);

        assert_eq!(history.summary_between(11, 12).hosts.len(), 2);
        assert_eq!(history.summary_between(10, 13).hosts.len(), 4);
        // A single day is a legitimate span, not an empty one.
        assert_eq!(history.summary_between(12, 12).hosts.len(), 1);
        assert_eq!(history.summary_between(12, 12).tunnel_down, 9);
    }

    #[test]
    fn a_span_outside_the_stored_days_is_empty_not_an_error() {
        let history = history_with_days(&[10, 11]);

        assert_eq!(history.summary_between(50, 60).tunnel_down, 0);
        // Reversed bounds must not panic on the BTreeMap range.
        assert_eq!(history.summary_between(11, 10).tunnel_down, 0);
    }

    #[test]
    fn the_rolling_presets_still_read_as_spans_ending_today() {
        let history = history_with_days(&[100, 99, 94, 80]);
        let today = 100;

        assert_eq!(
            history.summary(Range::Week, today).hosts.len(),
            history.summary_between(today - 6, today).hosts.len()
        );
        assert_eq!(
            history.summary(Range::Month, today).hosts.len(),
            history.summary_between(today - 29, today).hosts.len()
        );
    }

    #[test]
    fn the_recorded_span_is_the_outermost_days_with_data() {
        assert_eq!(history_with_days(&[7, 3, 9]).recorded_span(), Some((3, 9)));
        assert_eq!(History::default().recorded_span(), None);
    }
}
