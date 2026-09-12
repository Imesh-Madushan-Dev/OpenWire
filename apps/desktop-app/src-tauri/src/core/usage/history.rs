//! Rolling usage history.
//!
//! Three timeline tiers plus per-day breakdowns. The tiers exist so a month of
//! history costs a few thousand rows instead of two million: seconds are kept
//! only long enough for the live view, and everything older is already rolled
//! up by the time it is asked for.

use std::collections::{BTreeMap, HashMap, VecDeque};

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
pub const OTHER_KEY: &str = "Other";

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayBreakdown {
    pub up: u64,
    pub down: u64,
    pub tunnel_up: u64,
    pub tunnel_down: u64,
    pub apps: HashMap<String, Pair>,
    pub hosts: HashMap<String, Pair>,
    pub kinds: HashMap<String, Pair>,
}

impl DayBreakdown {
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
    days: BTreeMap<i64, DayBreakdown>,

    /// Executable path per app name, so an icon can still be shown for an app
    /// that only appears in history from a previous run.
    app_paths: HashMap<String, String>,

    /// The buckets still being filled. Persisted: skipping them threw away up
    /// to a minute of the 24-hour tier and up to an hour of the week/month
    /// tiers on every restart, which made those graphs look empty after a
    /// relaunch.
    minute_acc: Option<Sample>,
    hour_acc: Option<Sample>,
}

impl History {
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
        let day = self.days.entry(local_day).or_default();
        day.tunnel_up += up;
        day.tunnel_down += down;
        day.apps.entry(app.to_string()).or_default().add(up, down);
        day.hosts.entry(host.to_string()).or_default().add(up, down);
        day.kinds.entry(kind.to_string()).or_default().add(up, down);

        DayBreakdown::compact(&mut day.apps, MAX_APPS_PER_DAY);
        DayBreakdown::compact(&mut day.hosts, MAX_HOSTS_PER_DAY);
    }

    /// Records system-wide totals against a day, whether or not they were ours.
    pub fn record_day_totals(&mut self, local_day: i64, up: u64, down: u64) {
        let day = self.days.entry(local_day).or_default();
        day.up += up;
        day.down += down;
    }

    pub fn app_paths(&self) -> &HashMap<String, String> {
        &self.app_paths
    }

    pub fn set_app_paths(&mut self, paths: HashMap<String, String>) {
        self.app_paths = paths;
    }

    /// Drops anything past the retention window.
    pub fn prune(&mut self, today: i64) {
        let cutoff = today - DAYS_KEPT as i64 + 1;
        self.days.retain(|day, _| *day >= cutoff);
        trim(&mut self.minutes, MINUTES_KEPT);
        trim(&mut self.hours, HOURS_KEPT);

        // Paths are only worth keeping for apps a retained day still names.
        let days = &self.days;
        self.app_paths
            .retain(|app, _| days.values().any(|day| day.apps.contains_key(app)));
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
        let mut merged = DayBreakdown::default();
        if last < first {
            return merged;
        }

        for (_, day) in self.days.range(first..=last) {
            merged.up += day.up;
            merged.down += day.down;
            merged.tunnel_up += day.tunnel_up;
            merged.tunnel_down += day.tunnel_down;
            merge_into(&mut merged.apps, &day.apps);
            merge_into(&mut merged.hosts, &day.hosts);
            merge_into(&mut merged.kinds, &day.kinds);
        }

        merged
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
        let after: History = serde_json::from_str(&json).expect("deserialize");

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
