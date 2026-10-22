//! Per-second aggregation of what the kernel reports.
//!
//! The ETW callback can fire tens of thousands of times a second during a
//! download, so it does the least possible: one hash update per packet into a
//! [`FlowTable`]. Once a second the table is drained and [`fold`] turns its
//! flows into rows by app, host and traffic type - the only shape the app
//! stores - which is where every name lookup happens.

use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::Arc;

use super::parse::{Direction, Packet, Proto};
use super::Row;
use crate::core::usage::TrafficKind;

/// How an app is shown: an executable name, plus its full path when known,
/// which the app needs to extract an icon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppName {
    pub name: Arc<str>,
    pub path: Option<Arc<str>>,
}

impl AppName {
    pub fn named(name: &str) -> Self {
        Self {
            name: Arc::from(name),
            path: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FlowKey {
    pub pid: u32,
    pub proto: Proto,
    pub daddr: IpAddr,
    pub dport: u16,
    pub saddr: IpAddr,
    pub sport: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Bytes {
    pub up: u64,
    pub down: u64,
}

/// A pid stays named this many drains after its last packet. Long enough that
/// a quiet connection is not re-resolved each second, short enough that a
/// recycled pid is not credited to the process that used to own it.
const NAME_TTL_TICKS: u64 = 15;

#[derive(Debug, Default)]
pub struct FlowTable {
    flows: HashMap<FlowKey, Bytes>,
    names: HashMap<u32, (AppName, u64)>,
    tick: u64,
}

impl FlowTable {
    /// Counts one packet. `resolve` names a pid on first sight, here on the
    /// event thread rather than at the next drain: a short-lived process may
    /// be gone a second from now, and with it any chance of naming it.
    pub fn add(&mut self, packet: &Packet, resolve: impl FnOnce(u32) -> AppName) {
        // The proxy hop (app -> OpenWire) is loopback, and counting it would put
        // every tunnelled byte on the books twice.
        if packet.daddr.is_loopback() || packet.saddr.is_loopback() || packet.daddr.is_unspecified()
        {
            return;
        }

        let tick = self.tick;
        self.names
            .entry(packet.pid)
            .and_modify(|(_, seen)| *seen = tick)
            .or_insert_with(|| (resolve(packet.pid), tick));

        let bytes = self
            .flows
            .entry(FlowKey {
                pid: packet.pid,
                proto: packet.proto,
                daddr: packet.daddr,
                dport: packet.dport,
                saddr: packet.saddr,
                sport: packet.sport,
            })
            .or_default();
        match packet.direction {
            Direction::Send => bytes.up += u64::from(packet.size),
            Direction::Recv => bytes.down += u64::from(packet.size),
        }
    }

    /// Takes the last second's flows, each with its app already named.
    pub fn drain(&mut self) -> Vec<(AppName, FlowKey, Bytes)> {
        let flows = std::mem::take(&mut self.flows);
        let out = flows
            .into_iter()
            .map(|(key, bytes)| {
                let app = self
                    .names
                    .get(&key.pid)
                    .map(|(name, _)| name.clone())
                    .unwrap_or_else(|| AppName::named(crate::core::usage::process::UNKNOWN_APP));
                (app, key, bytes)
            })
            .collect();

        self.tick += 1;
        let tick = self.tick;
        self.names
            .retain(|_, (_, seen)| tick.saturating_sub(*seen) <= NAME_TTL_TICKS);
        out
    }
}

/// One row's identity while folding: app, host, traffic-type label.
type RowKey = (Arc<str>, String, &'static str);
/// A pending row's identity: quarter-hour bucket, app, host, traffic type.
type PendingKey = (i64, String, String, String);
/// What a row carries besides its identity and bytes: the app's path and the
/// host's country.
type Extras<P, C> = (Option<P>, Option<C>);

/// Folds drained flows into rows.
///
/// - `excluded` is OpenWire's own processes: their traffic is the tunnel and the
///   direct relay, which the relay has already attributed to the real apps.
/// - `local` is this machine's addresses, which decide the remote end. The
///   event fields are not relied on for that, so a provider that reports the
///   packet's direction rather than the connection's cannot flip host and
///   port into nonsense.
///
/// The remote address is located here because this is the last point it is
/// known: rows are keyed by host name, and one name can front many addresses.
/// The first address to place the host wins for the second.
pub fn fold(
    flows: Vec<(AppName, FlowKey, Bytes)>,
    t: i64,
    excluded: &HashSet<u32>,
    local: &HashSet<IpAddr>,
    mut host_of: impl FnMut(&IpAddr) -> Option<Arc<str>>,
) -> Vec<Row> {
    let mut rows: HashMap<RowKey, (Extras<Arc<str>, &'static str>, Bytes)> = HashMap::new();

    for (app, key, bytes) in flows {
        if excluded.contains(&key.pid) {
            continue;
        }

        let d_is_local = local.contains(&key.daddr);
        let s_is_local = local.contains(&key.saddr);
        let (remote, remote_port, local_port) = if d_is_local && !s_is_local {
            (key.saddr, key.sport, key.dport)
        } else {
            (key.daddr, key.dport, key.sport)
        };

        let host = host_of(&remote)
            .map(|name| name.to_string())
            .unwrap_or_else(|| remote.to_string());
        let kind = TrafficKind::classify(key.proto == Proto::Udp, remote_port, local_port).label();

        let ((_, country), total) = rows
            .entry((Arc::clone(&app.name), host, kind))
            .or_insert(((app.path.clone(), None), Bytes::default()));
        if country.is_none() {
            *country = crate::core::geo::country(remote);
        }
        total.up += bytes.up;
        total.down += bytes.down;
    }

    rows.into_iter()
        .map(|((app, host, kind), ((path, country), bytes))| Row {
            t,
            app: app.to_string(),
            path: path.map(|path| path.to_string()),
            host,
            country: country.map(str::to_string),
            kind: kind.to_string(),
            up: bytes.up,
            down: bytes.down,
        })
        .collect()
}

/// Rows waiting for a client: the app may be closed for hours while the
/// service keeps counting. Folded into quarter-hour buckets - every real
/// timezone offset is a multiple of 15 minutes, so no row can slide across
/// local midnight into the wrong day - and capped, past which hosts are
/// merged so memory stays bounded however long nobody listens.
#[derive(Debug, Default)]
pub struct Pending {
    rows: HashMap<PendingKey, (Extras<String, String>, Bytes)>,
}

const PENDING_BUCKET_SECS: i64 = 15 * 60;
const PENDING_CAP: usize = 20_000;

impl Pending {
    pub fn add(&mut self, rows: Vec<Row>) {
        for row in rows {
            let bucket = row.t - row.t.rem_euclid(PENDING_BUCKET_SECS);
            let host = if self.rows.len() >= PENDING_CAP {
                crate::core::usage::history::OTHER_KEY.to_string()
            } else {
                row.host
            };
            let ((_, country), total) = self
                .rows
                .entry((bucket, row.app, host, row.kind))
                .or_insert(((row.path, None), Bytes::default()));
            if country.is_none() {
                *country = row.country;
            }
            total.up += row.up;
            total.down += row.down;
        }
    }

    pub fn take(&mut self) -> Vec<Row> {
        std::mem::take(&mut self.rows)
            .into_iter()
            .map(|((t, app, host, kind), ((path, country), bytes))| Row {
                t,
                app,
                path,
                // A host merged into `Other` has no one country.
                country: country.filter(|_| host != crate::core::usage::history::OTHER_KEY),
                host,
                kind,
                up: bytes.up,
                down: bytes.down,
            })
            .collect()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.rows.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    fn packet(
        pid: u32,
        direction: Direction,
        size: u32,
        d: &str,
        dport: u16,
        s: &str,
        sport: u16,
    ) -> Packet {
        Packet {
            pid,
            size,
            proto: Proto::Tcp,
            direction,
            daddr: ip(d),
            dport,
            saddr: ip(s),
            sport,
        }
    }

    fn named(name: &'static str) -> impl FnOnce(u32) -> AppName {
        move |_| AppName {
            name: Arc::from(name),
            path: Some(Arc::from(format!(r"C:\Apps\{name}").as_str())),
        }
    }

    #[test]
    fn sends_and_receives_on_one_connection_land_in_one_row() {
        let mut table = FlowTable::default();
        table.add(
            &packet(
                7,
                Direction::Send,
                100,
                "93.184.216.34",
                443,
                "192.168.1.5",
                50000,
            ),
            named("edge.exe"),
        );
        table.add(
            &packet(
                7,
                Direction::Recv,
                900,
                "93.184.216.34",
                443,
                "192.168.1.5",
                50000,
            ),
            named("unused.exe"),
        );

        let local = HashSet::from([ip("192.168.1.5")]);
        let rows = fold(table.drain(), 42, &HashSet::new(), &local, |address| {
            (address == &ip("93.184.216.34")).then(|| Arc::from("example.com"))
        });

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.app, "edge.exe", "named once, at first sight");
        assert_eq!(row.path.as_deref(), Some(r"C:\Apps\edge.exe"));
        assert_eq!(row.host, "example.com");
        assert_eq!(
            row.country.as_deref().map(str::len),
            Some(2),
            "located from the address, which the row no longer carries"
        );
        assert_eq!(row.kind, "HTTPS");
        assert_eq!((row.up, row.down, row.t), (100, 900, 42));
    }

    #[test]
    fn the_remote_end_is_whichever_address_is_not_ours() {
        let mut table = FlowTable::default();
        // Reported packet-wise: the destination is this machine.
        table.add(
            &packet(9, Direction::Recv, 500, "10.0.0.2", 51000, "8.8.8.8", 53),
            named("svc.exe"),
        );

        let local = HashSet::from([ip("10.0.0.2")]);
        let rows = fold(table.drain(), 0, &HashSet::new(), &local, |_| None);

        assert_eq!(
            rows[0].host, "8.8.8.8",
            "unresolved hosts show their address"
        );
        assert_eq!(rows[0].kind, "DNS", "classified by the remote port");
    }

    #[test]
    fn loopback_and_openwire_itself_are_left_to_the_relay() {
        let mut table = FlowTable::default();
        table.add(
            &packet(
                3,
                Direction::Send,
                10,
                "127.0.0.1",
                10808,
                "127.0.0.1",
                50001,
            ),
            named("chrome.exe"),
        );
        table.add(
            &packet(
                4,
                Direction::Send,
                10,
                "203.0.113.9",
                443,
                "10.0.0.2",
                50002,
            ),
            named("openwire.exe"),
        );

        let rows = fold(
            table.drain(),
            0,
            &HashSet::from([4]),
            &HashSet::new(),
            |_| None,
        );
        assert!(rows.is_empty(), "{rows:?}");
    }

    #[test]
    fn names_expire_so_a_recycled_pid_is_resolved_afresh() {
        let mut table = FlowTable::default();
        table.add(
            &packet(5, Direction::Send, 1, "1.1.1.1", 443, "10.0.0.2", 1),
            named("old.exe"),
        );
        for _ in 0..=NAME_TTL_TICKS + 1 {
            table.drain();
        }
        table.add(
            &packet(5, Direction::Send, 1, "1.1.1.1", 443, "10.0.0.2", 1),
            named("new.exe"),
        );

        let rows = fold(table.drain(), 0, &HashSet::new(), &HashSet::new(), |_| None);
        assert_eq!(rows[0].app, "new.exe");
    }

    #[test]
    fn pending_rows_fold_into_quarter_hours_without_losing_bytes() {
        let row = |t, down| Row {
            t,
            app: "a.exe".into(),
            path: None,
            host: "h".into(),
            country: None,
            kind: "HTTPS".into(),
            up: 0,
            down,
        };

        let mut pending = Pending::default();
        pending.add(vec![row(1_000, 5), row(1_100, 7), row(2_000, 1)]);

        let mut rows = pending.take();
        rows.sort_by_key(|row| row.t);
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].t, rows[0].down), (900, 12));
        assert_eq!((rows[1].t, rows[1].down), (1_800, 1));
        assert_eq!(pending.len(), 0);
    }
}
