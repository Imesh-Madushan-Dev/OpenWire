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

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::core::metrics::Metrics;
use crate::core::store;
use crate::error::AppResult;

use history::{rank, DayBreakdown, History, Pair, Range, Sample};
use netstat::InterfaceCounters;
use process::ProcessResolver;
use srum::AppBytes;

/// How many entries each ranked column returns.
const TOP_N: usize = 12;

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
            _ => Self::Other,
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
}

impl UsageTracker {
    pub fn load(path: &Path) -> Self {
        let history: History = store::read_json(path).ok().flatten().unwrap_or_default();

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
        let merged = self
            .history
            .lock()
            .map(|history| history.summary(range, today))
            .unwrap_or_default();

        self.assemble(merged, today - range.days() as i64 + 1, today)
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

        let merged = self
            .history
            .lock()
            .map(|history| history.summary_between(first, last))
            .unwrap_or_default();

        self.assemble(merged, first, last)
    }

    /// First and last day holding any record, as timestamps.
    pub fn recorded_span(&self) -> Option<(i64, i64)> {
        let (first, last) = self.history.lock().ok()?.recorded_span()?;
        Some((netstat::day_noon_unix(first), netstat::day_noon_unix(last)))
    }

    fn assemble(&self, merged: DayBreakdown, first_day: i64, last_day: i64) -> UsageSummary {
        UsageSummary {
            from_ts: netstat::day_noon_unix(first_day),
            to_ts: netstat::day_noon_unix(last_day),
            total_up: merged.up,
            total_down: merged.down,
            tunnel_up: merged.tunnel_up,
            tunnel_down: merged.tunnel_down,
            apps: self.with_icons(to_entries(&merged.apps)),
            hosts: to_entries(&merged.hosts),
            kinds: to_entries(&merged.kinds),
            attribution_supported: cfg!(windows),
        }
    }

    /// Device-wide per-app usage for a period, from the OS's own accounting.
    ///
    /// Blocking and slow - seconds - so callers must keep it off the UI thread.
    /// The result is cached per day span: a past period cannot change, and the
    /// current one is refreshed only when asked for again after a new query.
    pub fn device_apps(&self, from_ts: i64, to_ts: i64) -> Vec<UsageEntry> {
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
            return Vec::new();
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

    fn to_device_entries(&self, rows: Vec<AppBytes>) -> Vec<UsageEntry> {
        // The paths come from the OS, so the icon cache can be primed with
        // apps that never touched the tunnel.
        self.resolver.seed_paths(
            rows.iter()
                .filter_map(|row| row.path.clone().map(|path| (row.name.clone(), path)))
                .collect(),
        );

        let entries = rows
            .into_iter()
            .take(TOP_N)
            .map(|row| UsageEntry {
                key: row.name,
                up: row.up,
                down: row.down,
                total: row.up + row.down,
                icon: None,
            })
            .collect();

        self.with_icons(entries)
    }

    /// Attaches each app's executable icon to its row.
    fn with_icons(&self, mut entries: Vec<UsageEntry>) -> Vec<UsageEntry> {
        for entry in &mut entries {
            entry.icon = self.resolver.icon_for_app(&entry.key);
        }
        entries
    }

    /// Trims to the retention window and writes to disk.
    pub fn persist(&self, path: &Path) -> AppResult<()> {
        let Ok(mut history) = self.history.lock() else {
            return Ok(());
        };
        history.set_app_paths(self.resolver.known_paths());
        history.prune(netstat::local_day(unix_seconds()));
        store::write_json(path, &*history)
    }
}

fn to_entries(map: &HashMap<String, Pair>) -> Vec<UsageEntry> {
    rank(map, TOP_N)
        .into_iter()
        .map(|(key, pair)| UsageEntry {
            key,
            up: pair.up,
            down: pair.down,
            total: pair.total(),
            icon: None,
        })
        .collect()
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
    /// PNG data URL of the app's executable icon. Only ever set on the apps
    /// column, and only where the exe could be located and read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
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
    pub apps: Vec<UsageEntry>,
    pub hosts: Vec<UsageEntry>,
    pub kinds: Vec<UsageEntry>,
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
            },
            Arc::clone(&metrics),
        );
        handle.recorder.add_down(900);
        handle.recorder.add_up(100);

        tracker.sample();

        let summary = tracker.summary(Range::Day);
        assert_eq!(summary.tunnel_down, 900);
        assert_eq!(summary.tunnel_up, 100);
        assert_eq!(summary.apps[0].key, "edge.exe");
        assert_eq!(summary.apps[0].total, 1_000);
        assert_eq!(summary.hosts[0].key, "example.com");
        assert_eq!(summary.kinds[0].key, "HTTPS");

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
                },
                metrics,
            );
            handle.recorder.add_down(512);
            // Dropped without an intervening sample: the residual has to survive.
        }

        tracker.sample();

        let summary = tracker.summary(Range::Day);
        assert_eq!(summary.tunnel_down, 512, "residual bytes were dropped");
        assert_eq!(summary.hosts[0].key, "example.net");
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
            },
            metrics,
        );
        handle.recorder.add_down(10_000);

        let sample = tracker.sample();
        assert!(sample.down >= sample.tunnel_down);
    }
}
