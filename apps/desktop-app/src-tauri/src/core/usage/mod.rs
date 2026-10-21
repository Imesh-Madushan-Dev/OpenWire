//! Network usage accounting.
//!
//! The hot path is two atomics per relayed buffer. Everything expensive - the
//! interface counters, the day bucketing, the ranked maps - happens once a
//! second on the sampler, so throughput accounting never sits between the user
//! and their packets.

pub mod history;
pub mod icon;
pub mod netstat;
pub mod process;
pub mod srum;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::core::geo;
use crate::core::metrics::Metrics;
use crate::core::netmon;
use crate::core::store;
use crate::error::AppResult;

use history::{rank, Attributed, DayBreakdown, History, Pair, Range, Sample};
use netstat::InterfaceCounters;
use process::ProcessResolver;
use srum::AppBytes;

/// How many entries each ranked column returns. The UI shows the first few
/// and expands to the rest; anything past this is summed into `more`.
const TOP_N: usize = 50;

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

/// What a connection is *for*, inferred from its destination port.
///
/// A guess, but a stable and useful one - and the only classification available
/// without inspecting payloads, which would defeat the point of the tunnel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficKind {
    Https,
    Http,
    Dns,
    Mail,
    Ssh,
    Media,
    Realtime,
    Transfer,
    /// HTTP/3 and other QUIC: HTTPS's ports, over UDP. Only the network
    /// monitor sees these - the proxy is TCP-only.
    Quic,
    Vpn,
    /// Devices announcing and finding each other on the local network.
    Discovery,
    TimeSync,
    Other,
}

impl TrafficKind {
    pub fn from_port(port: u16) -> Self {
        match port {
            443 | 8443 | 4443 => Self::Https,
            80 | 8080 | 8000 | 8008 => Self::Http,
            53 | 853 | 5353 => Self::Dns,
            25 | 110 | 143 | 465 | 587 | 993 | 995 => Self::Mail,
            22 => Self::Ssh,
            554 | 1935 | 1755 => Self::Media,
            3478..=3481 | 5349 | 19302..=19309 => Self::Realtime,
            20 | 21 | 139 | 445 | 873 | 2049 => Self::Transfer,
            500 | 4500 | 1194 | 1701 | 1723 | 51820 => Self::Vpn,
            137 | 138 | 1900 | 3702 | 5355 => Self::Discovery,
            123 => Self::TimeSync,
            _ => Self::Other,
        }
    }

    /// Classifies one flow seen by the network monitor. The remote port names
    /// the service in the usual case; when it is an ephemeral one, this
    /// machine is the server and its own port is the meaningful one.
    pub fn classify(udp: bool, remote_port: u16, local_port: u16) -> Self {
        let by_remote = Self::from_port(remote_port);
        let kind = if by_remote == Self::Other && remote_port >= 49152 {
            Self::from_port(local_port)
        } else {
            by_remote
        };

        match kind {
            Self::Https if udp => Self::Quic,
            kind => kind,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Https => "HTTPS",
            Self::Http => "HTTP",
            Self::Dns => "DNS",
            Self::Mail => "Email",
            Self::Ssh => "SSH",
            Self::Media => "Streaming",
            Self::Realtime => "Real-time",
            Self::Transfer => "File transfer",
            Self::Quic => "QUIC",
            Self::Vpn => "VPN",
            Self::Discovery => "Local discovery",
            Self::TimeSync => "Time sync",
            Self::Other => "Other",
        }
    }
}

/// Everything one connection's bytes get attributed to.
#[derive(Debug, Clone)]
pub struct UsageKey {
    pub app: String,
    pub host: String,
    pub kind: TrafficKind,
    /// Where the host is, when the address it was reached at is known: a
    /// direct connection, or a host that is itself an address. Through the
    /// tunnel the server resolves the name, so this side never learns it.
    pub country: Option<&'static str>,
}

// ---------------------------------------------------------------------------
// Per-connection recording
// ---------------------------------------------------------------------------

/// Written by the relay loops; drained by the sampler.
#[derive(Debug)]
pub struct ConnRecorder {
    up: AtomicU64,
    down: AtomicU64,
    metrics: Arc<Metrics>,
}

impl ConnRecorder {
    fn new(metrics: Arc<Metrics>) -> Self {
        Self {
            up: AtomicU64::new(0),
            down: AtomicU64::new(0),
            metrics,
        }
    }

    pub fn add_up(&self, bytes: u64) {
        self.up.fetch_add(bytes, Ordering::Relaxed);
        self.metrics.add_up(bytes);
    }

    pub fn add_down(&self, bytes: u64) {
        self.down.fetch_add(bytes, Ordering::Relaxed);
        self.metrics.add_down(bytes);
    }

    fn drain(&self) -> (u64, u64) {
        (
            self.up.swap(0, Ordering::Relaxed),
            self.down.swap(0, Ordering::Relaxed),
        )
    }
}

/// Deregisters its connection when dropped, so a panicking relay cannot leak an
/// entry into the live set.
pub struct ConnHandle {
    id: u64,
    tracker: Arc<UsageTracker>,
    pub recorder: Arc<ConnRecorder>,
}

impl Drop for ConnHandle {
    fn drop(&mut self) {
        self.tracker.close(self.id);
    }
}

struct LiveConn {
    key: UsageKey,
    recorder: Arc<ConnRecorder>,
}

// ---------------------------------------------------------------------------
// Tracker
// ---------------------------------------------------------------------------

pub struct UsageTracker {
    live: Mutex<HashMap<u64, LiveConn>>,
    /// Device-wide per-app figures, keyed by the day span they cover. Windows
    /// takes seconds to answer, so an answer is kept until the day rolls over.
    device_apps: Mutex<HashMap<(i64, i64), Vec<AppBytes>>>,
    /// Residual bytes from connections that closed between samples.
    drained: Mutex<Vec<(UsageKey, u64, u64)>>,
    next_id: AtomicU64,
    history: Mutex<History>,
    interfaces: Mutex<InterfaceCounters>,
    resolver: ProcessResolver,
    /// One file per local day, beside the main store.
    days_dir: PathBuf,
}

impl UsageTracker {
    pub fn load(path: &Path) -> Self {
        let mut history: History = store::read_json(path).ok().flatten().unwrap_or_default();
        let days_dir = path.with_file_name("usage-days");
        history.adopt_days(read_days(&days_dir));

        let resolver = ProcessResolver::new();
        resolver.seed_paths(history.app_paths().clone());

        Self {
            live: Mutex::new(HashMap::new()),
            device_apps: Mutex::new(HashMap::new()),
            drained: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
            history: Mutex::new(history),
            interfaces: Mutex::new(InterfaceCounters::new()),
            resolver,
            days_dir,
        }
    }

    /// Records what the network monitor saw outside Maizy.
    pub fn record_monitored(&self, rows: &[netmon::Row]) {
        // The monitor names apps from their full image path, so icons work
        // for apps that never touched the proxy.
        let paths: HashMap<String, String> = rows
            .iter()
            .filter_map(|row| {
                row.path
                    .as_ref()
                    .map(|path| (row.app.clone(), path.clone()))
            })
            .collect();
        if !paths.is_empty() {
            self.resolver.seed_paths(paths);
        }

        if let Ok(mut history) = self.history.lock() {
            for row in rows {
                let day = netstat::local_day(row.t);
                history.record_monitored(day, &row.app, &row.host, &row.kind, row.up, row.down);
                if let Some(country) = row.country.as_deref() {
                    history.record_country(day, &row.host, country);
                }
            }
        }
    }

    /// Names the process behind a loopback client port.
    pub fn app_for_port(&self, local_port: u16) -> String {
        self.resolver
            .app_for_port(local_port)
            .unwrap_or_else(|| process::UNKNOWN_APP.to_string())
    }

    pub fn open(self: &Arc<Self>, key: UsageKey, metrics: Arc<Metrics>) -> ConnHandle {
        let recorder = Arc::new(ConnRecorder::new(metrics));
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        if let Ok(mut live) = self.live.lock() {
            live.insert(
                id,
                LiveConn {
                    key,
                    recorder: Arc::clone(&recorder),
                },
            );
        }

        ConnHandle {
            id,
            tracker: Arc::clone(self),
            recorder,
        }
    }

    fn close(&self, id: u64) {
        let Some(conn) = self.live.lock().ok().and_then(|mut live| live.remove(&id)) else {
            return;
        };

        // Hand the last partial second to the sampler rather than recording it
        // here, so every byte lands in exactly one timeline bucket.
        let (up, down) = conn.recorder.drain();
        if up | down != 0 {
            if let Ok(mut drained) = self.drained.lock() {
                drained.push((conn.key, up, down));
            }
        }
    }

    /// Called once a second. Folds the last second of traffic into history and
    /// returns it for the live graph.
    pub fn sample(&self) -> Sample {
        let now = unix_seconds();

        let (system_up, system_down) = self
            .interfaces
            .lock()
            .map(|mut counters| counters.delta())
            .unwrap_or((0, 0));

        let mut deltas: Vec<(UsageKey, u64, u64)> = self
            .drained
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default();

        if let Ok(live) = self.live.lock() {
            for conn in live.values() {
                let (up, down) = conn.recorder.drain();
                if up | down != 0 {
                    deltas.push((conn.key.clone(), up, down));
                }
            }
        }

        let tunnel_up: u64 = deltas.iter().map(|(_, up, _)| *up).sum();
        let tunnel_down: u64 = deltas.iter().map(|(_, _, down)| *down).sum();

        let sample = Sample {
            t: now,
            // Where interface counters are unavailable the tunnel is all we can
            // see; `max` keeps "tunnel <= total" true rather than reporting a
            // subset larger than the whole.
            up: system_up.max(tunnel_up),
            down: system_down.max(tunnel_down),
            tunnel_up,
            tunnel_down,
        };

        if let Ok(mut history) = self.history.lock() {
            history.push(sample);

            let day = netstat::local_day(now);
            history.record_day_totals(day, sample.up, sample.down);
            for (key, up, down) in deltas {
                history.record_breakdown(day, &key.app, &key.host, key.kind.label(), up, down);
                if let Some(country) = key.country {
                    history.record_country(day, &key.host, country);
                }
            }
        }

        sample
    }

    pub fn timeline(&self, range: Range) -> Vec<Sample> {
        self.history
            .lock()
            .map(|history| history.timeline(range))
            .unwrap_or_default()
    }

    pub fn summary(&self, range: Range) -> UsageSummary {
        let today = netstat::local_day(unix_seconds());
        let first = today - range.days() as i64 + 1;
        let (merged, monitored_from) = self
            .history
            .lock()
            .map(|history| {
                (
                    history.summary(range, today),
                    history.first_monitored_day(first, today),
                )
            })
            .unwrap_or_default();

        self.assemble(merged, monitored_from, first, today)
    }

    /// Summary for an arbitrary period, given any instant inside the first and
    /// last day wanted.
    ///
    /// Taking timestamps rather than calendar dates keeps one definition of
    /// "which day is this" - the same `local_day` the sampler buckets with - so
    /// the UI never has to reimplement the host's timezone rules.
    pub fn summary_between(&self, from_ts: i64, to_ts: i64) -> UsageSummary {
        let (first, last) = {
            let (a, b) = (netstat::local_day(from_ts), netstat::local_day(to_ts));
            if a <= b {
                (a, b)
            } else {
                (b, a)
            }
        };

        let (merged, monitored_from) = self
            .history
            .lock()
            .map(|history| {
                (
                    history.summary_between(first, last),
                    history.first_monitored_day(first, last),
                )
            })
            .unwrap_or_default();

        self.assemble(merged, monitored_from, first, last)
    }

    /// First and last day holding any record, as timestamps.
    pub fn recorded_span(&self) -> Option<(i64, i64)> {
        let (first, last) = self.history.lock().ok()?.recorded_span()?;
        Some((netstat::day_noon_unix(first), netstat::day_noon_unix(last)))
    }

    fn assemble(
        &self,
        merged: DayBreakdown,
        monitored_from: Option<i64>,
        first_day: i64,
        last_day: i64,
    ) -> UsageSummary {
        let relay = Attributed {
            up: merged.tunnel_up,
            down: merged.tunnel_down,
            apps: merged.apps,
            hosts: merged.hosts,
            kinds: merged.kinds,
        };

        // The device view is both sources together. They never overlap: the
        // monitor leaves Maizy's own process out, and that process is exactly
        // where the relay's traffic would otherwise appear.
        let device = (!merged.monitored.is_empty()).then(|| {
            let mut all = merged.monitored;
            for (target, source) in [
                (&mut all.apps, &relay.apps),
                (&mut all.hosts, &relay.hosts),
                (&mut all.kinds, &relay.kinds),
            ] {
                for (key, pair) in source {
                    let slot = target.entry(key.clone()).or_default();
                    slot.up += pair.up;
                    slot.down += pair.down;
                }
            }
            let mut columns = Columns::from(&all);
            columns.hosts.locate(&merged.countries);
            columns
        });

        let mut maizy = Columns::from(&relay);
        maizy.hosts.locate(&merged.countries);

        UsageSummary {
            from_ts: netstat::day_noon_unix(first_day),
            to_ts: netstat::day_noon_unix(last_day),
            total_up: merged.up,
            total_down: merged.down,
            tunnel_up: merged.tunnel_up,
            tunnel_down: merged.tunnel_down,
            maizy,
            device,
            monitored_from_ts: monitored_from.map(netstat::day_noon_unix),
            attribution_supported: cfg!(windows),
        }
    }

    /// Device-wide per-app usage for a period, from the OS's own accounting.
    ///
    /// Blocking and slow - seconds - so callers must keep it off the UI thread.
    /// The result is cached per day span: a past period cannot change, and the
    /// current one is refreshed only when asked for again after a new query.
    pub fn device_apps(&self, from_ts: i64, to_ts: i64) -> Ranked {
        let key = (
            netstat::local_day(from_ts.min(to_ts)),
            netstat::local_day(to_ts.max(from_ts)),
        );

        if let Some(cached) = self
            .device_apps
            .lock()
            .ok()
            .and_then(|cache| cache.get(&key).cloned())
        {
            return self.to_device_entries(cached);
        }

        // Queried on the span the caller asked for, but keyed by whole days, so
        // a period's figures are fetched once however often the UI asks.
        let Some(rows) = srum::per_app(from_ts.min(to_ts), to_ts.max(from_ts)) else {
            return Ranked::default();
        };

        if let Ok(mut cache) = self.device_apps.lock() {
            // One entry per span the user visits; a handful at most, and the
            // whole thing is dropped when the app closes.
            if cache.len() > 24 {
                cache.clear();
            }
            cache.insert(key, rows.clone());
        }

        self.to_device_entries(rows)
    }

    fn to_device_entries(&self, rows: Vec<AppBytes>) -> Ranked {
        // The paths come from the OS, so the icon cache can be primed with
        // apps that never touched the tunnel.
        self.resolver.seed_paths(
            rows.iter()
                .filter_map(|row| row.path.clone().map(|path| (row.name.clone(), path)))
                .collect(),
        );

        // Already sorted largest first by the reader.
        let mut entries: Vec<UsageEntry> = rows
            .into_iter()
            .map(|row| UsageEntry::new(row.name, row.up, row.down))
            .collect();
        let rest = entries.split_off(entries.len().min(TOP_N));
        Ranked::new(entries, rest)
    }

    /// Icons for the named apps, where the executable is known and has one.
    ///
    /// Asked for separately rather than embedded in every summary: the
    /// summary is polled, and a few KB of PNG per row per poll adds up to
    /// megabytes a minute crossing the IPC bridge for pictures that never
    /// change.
    pub fn icons(&self, apps: &[String]) -> HashMap<String, String> {
        apps.iter()
            .take(2 * TOP_N)
            .filter_map(|app| {
                self.resolver
                    .icon_for_app(app)
                    .map(|icon| (app.clone(), icon))
            })
            .collect()
    }

    /// Trims to the retention window and writes to disk: the timeline tiers
    /// to the main file, and only the days that changed to their own files.
    pub fn persist(&self, path: &Path) -> AppResult<()> {
        let today = netstat::local_day(unix_seconds());
        let dirty = {
            let Ok(mut history) = self.history.lock() else {
                return Ok(());
            };
            history.set_app_paths(self.resolver.known_paths());
            history.prune(today);
            store::write_json(path, &*history)?;
            history.take_dirty()
        };

        // Written outside the lock: the sampler takes it every second.
        let mut failed = Vec::new();
        for (day, breakdown) in dirty {
            let file = self.days_dir.join(format!("{day}.json"));
            if let Err(err) = store::write_json_compact(&file, &breakdown) {
                log::warn!("could not save usage for day {day}: {err}");
                failed.push(day);
            }
        }
        if !failed.is_empty() {
            if let Ok(mut history) = self.history.lock() {
                history.mark_dirty(failed);
            }
        }

        remove_expired_days(&self.days_dir, History::retention_cutoff(today));
        Ok(())
    }
}

/// Every day file in `dir`. A file that will not parse is skipped rather than
/// failing the load - one bad day must not cost the other eighty-nine.
fn read_days(dir: &Path) -> BTreeMap<i64, DayBreakdown> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return BTreeMap::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let day: i64 = path.file_stem()?.to_str()?.parse().ok()?;
            let breakdown = store::read_json::<DayBreakdown>(&path).ok().flatten()?;
            Some((day, breakdown))
        })
        .collect()
}

fn remove_expired_days(dir: &Path, cutoff: i64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let expired = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.parse::<i64>().ok())
            .is_some_and(|day| day < cutoff);
        if expired {
            let _ = std::fs::remove_file(path);
        }
    }
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageEntry {
    pub key: String,
    pub up: u64,
    pub down: u64,
    pub total: u64,
    /// ISO country code, on hosts only, when it is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
}

impl UsageEntry {
    fn new(key: String, up: u64, down: u64) -> Self {
        Self {
            key,
            up,
            down,
            total: up + down,
            country: None,
        }
    }
}

/// The tail past the listed entries, summed: "+41 more, 189 MB".
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct More {
    pub count: usize,
    pub up: u64,
    pub down: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ranked {
    pub entries: Vec<UsageEntry>,
    pub more: Option<More>,
}

impl Ranked {
    fn new(entries: Vec<UsageEntry>, rest: Vec<UsageEntry>) -> Self {
        let more = (!rest.is_empty()).then(|| More {
            count: rest.len(),
            up: rest.iter().map(|entry| entry.up).sum(),
            down: rest.iter().map(|entry| entry.down).sum(),
        });
        Self { entries, more }
    }

    fn from_map(map: &HashMap<String, Pair>) -> Self {
        let mut entries: Vec<UsageEntry> = rank(map, usize::MAX)
            .into_iter()
            .map(|(key, pair)| UsageEntry::new(key, pair.up, pair.down))
            .collect();
        let rest = entries.split_off(entries.len().min(TOP_N));
        Self::new(entries, rest)
    }

    /// Puts a country on each listed host: where it was last reached, or for
    /// a host that is itself an address, wherever that address is.
    fn locate(&mut self, countries: &HashMap<String, String>) {
        for entry in &mut self.entries {
            entry.country = countries
                .get(&entry.key)
                .cloned()
                .or_else(|| geo::country_of_literal(&entry.key).map(str::to_string));
        }
    }
}

/// The three ranked columns for one view of the traffic.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Columns {
    pub apps: Ranked,
    pub hosts: Ranked,
    pub kinds: Ranked,
}

impl From<&Attributed> for Columns {
    fn from(source: &Attributed) -> Self {
        Self {
            apps: Ranked::from_map(&source.apps),
            hosts: Ranked::from_map(&source.hosts),
            kinds: Ranked::from_map(&source.kinds),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    /// Midday of the first and last local day the figures cover. The UI labels
    /// the period from these rather than assuming it got what it asked for.
    pub from_ts: i64,
    pub to_ts: i64,
    pub total_up: u64,
    pub total_down: u64,
    pub tunnel_up: u64,
    pub tunnel_down: u64,
    /// Only what went through Maizy's proxy.
    pub maizy: Columns,
    /// Everything on this device: the network monitor's figures with the
    /// relay's folded in. `None` when the monitor recorded nothing in the
    /// period - it was not installed, or not yet.
    pub device: Option<Columns>,
    /// Midday of the first day in the period the monitor has figures for.
    pub monitored_from_ts: Option<i64>,
    /// False where per-app names cannot be resolved, so the UI can say why the
    /// Apps column is empty instead of looking broken.
    pub attribution_supported: bool,
}

pub fn unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_map_to_sensible_kinds() {
        assert_eq!(TrafficKind::from_port(443), TrafficKind::Https);
        assert_eq!(TrafficKind::from_port(80), TrafficKind::Http);
        assert_eq!(TrafficKind::from_port(53), TrafficKind::Dns);
        assert_eq!(TrafficKind::from_port(19302), TrafficKind::Realtime);
        assert_eq!(TrafficKind::from_port(61234), TrafficKind::Other);
    }

    fn tracker() -> Arc<UsageTracker> {
        Arc::new(UsageTracker::load(Path::new(
            "definitely-not-a-real-usage-file.json",
        )))
    }

    #[test]
    fn relayed_bytes_reach_the_summary_attributed() {
        let tracker = tracker();
        let metrics = Arc::new(Metrics::new());

        let handle = tracker.open(
            UsageKey {
                app: "edge.exe".to_string(),
                host: "example.com".to_string(),
                kind: TrafficKind::Https,
                country: None,
            },
            Arc::clone(&metrics),
        );
        handle.recorder.add_down(900);
        handle.recorder.add_up(100);

        tracker.sample();

        let summary = tracker.summary(Range::Day);
        assert_eq!(summary.tunnel_down, 900);
        assert_eq!(summary.tunnel_up, 100);
        assert_eq!(summary.maizy.apps.entries[0].key, "edge.exe");
        assert_eq!(summary.maizy.apps.entries[0].total, 1_000);
        assert_eq!(summary.maizy.hosts.entries[0].key, "example.com");
        assert_eq!(summary.maizy.kinds.entries[0].key, "HTTPS");
        assert!(summary.device.is_none(), "no monitor, no device view");

        // The global metrics must stay in step with per-connection accounting.
        assert_eq!(metrics.snapshot().down_bytes, 900);
    }

    #[test]
    fn bytes_written_just_before_close_are_not_lost() {
        let tracker = tracker();
        let metrics = Arc::new(Metrics::new());

        {
            let handle = tracker.open(
                UsageKey {
                    app: "curl.exe".to_string(),
                    host: "example.net".to_string(),
                    kind: TrafficKind::Https,
                    country: None,
                },
                metrics,
            );
            handle.recorder.add_down(512);
            // Dropped without an intervening sample: the residual has to survive.
        }

        tracker.sample();

        let summary = tracker.summary(Range::Day);
        assert_eq!(summary.tunnel_down, 512, "residual bytes were dropped");
        assert_eq!(summary.maizy.hosts.entries[0].key, "example.net");
    }

    #[test]
    fn the_device_view_is_the_monitor_and_the_relay_together() {
        let tracker = tracker();
        let metrics = Arc::new(Metrics::new());
        let handle = tracker.open(
            UsageKey {
                app: "edge.exe".to_string(),
                host: "example.com".to_string(),
                kind: TrafficKind::Https,
                country: None,
            },
            metrics,
        );
        handle.recorder.add_down(300);
        tracker.sample();

        let row = |app: &str, host: &str, down| netmon::Row {
            t: unix_seconds(),
            app: app.to_string(),
            path: None,
            host: host.to_string(),
            country: (host == "cdn.example").then(|| "SG".to_string()),
            kind: "HTTPS".to_string(),
            up: 0,
            down,
        };
        tracker.record_monitored(&[
            row("edge.exe", "example.com", 200),
            row("steam.exe", "cdn.example", 5_000),
        ]);

        let summary = tracker.summary(Range::Day);
        let device = summary.device.expect("monitor data makes a device view");
        assert_eq!(device.apps.entries[0].key, "steam.exe");
        let edge = device
            .apps
            .entries
            .iter()
            .find(|e| e.key == "edge.exe")
            .unwrap();
        assert_eq!(edge.down, 500, "relay and monitor bytes add up for one app");
        assert_eq!(
            summary.maizy.apps.entries.len(),
            1,
            "the Maizy view is untouched"
        );
        assert!(summary.monitored_from_ts.is_some());

        let cdn = device.hosts.entries.iter().find(|e| e.key == "cdn.example").unwrap();
        assert_eq!(cdn.country.as_deref(), Some("SG"), "the monitor's location reaches the column");
    }

    #[test]
    fn a_long_tail_is_summed_rather_than_dropped() {
        let map: HashMap<String, Pair> = (0..(TOP_N + 5))
            .map(|n| {
                (
                    format!("h{n}"),
                    Pair {
                        up: 1,
                        down: n as u64 + 1,
                    },
                )
            })
            .collect();
        let ranked = Ranked::from_map(&map);

        assert_eq!(ranked.entries.len(), TOP_N);
        let more = ranked.more.expect("a tail");
        assert_eq!(more.count, 5);
        assert_eq!(more.up, 5);
        assert_eq!(more.down, (1..=5).sum::<u64>(), "the smallest five");
    }

    #[test]
    fn quic_and_server_ports_classify_sensibly() {
        assert_eq!(TrafficKind::classify(true, 443, 50_000), TrafficKind::Quic);
        assert_eq!(
            TrafficKind::classify(false, 443, 50_000),
            TrafficKind::Https
        );
        // This machine serving SSH: the client's port is ephemeral.
        assert_eq!(TrafficKind::classify(false, 52_000, 22), TrafficKind::Ssh);
        assert_eq!(
            TrafficKind::classify(true, 1900, 1900),
            TrafficKind::Discovery
        );
    }

    #[test]
    fn a_sample_with_no_traffic_stays_empty() {
        let tracker = tracker();
        let sample = tracker.sample();
        assert_eq!(sample.tunnel_up, 0);
        assert_eq!(sample.tunnel_down, 0);

        // One point exists on the live timeline even when idle, so the graph
        // scrolls rather than freezing.
        assert_eq!(tracker.timeline(Range::FiveMinutes).len(), 1);
    }

    #[test]
    fn tunnel_never_exceeds_the_reported_total() {
        let tracker = tracker();
        let metrics = Arc::new(Metrics::new());
        let handle = tracker.open(
            UsageKey {
                app: "app".to_string(),
                host: "h".to_string(),
                kind: TrafficKind::Https,
                country: None,
            },
            metrics,
        );
        handle.recorder.add_down(10_000);

        let sample = tracker.sample();
        assert!(sample.down >= sample.tunnel_down);
    }
}
