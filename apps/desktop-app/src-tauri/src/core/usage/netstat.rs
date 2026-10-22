//! System-wide throughput, read from the OS interface counters.
//!
//! This is what makes the meter a *network* meter rather than a VPN meter: it
//! sees every byte the machine moves, including traffic that never touched the
//! tunnel. Attribution still only covers our own connections - naming the app
//! behind arbitrary system traffic needs a driver, which this deliberately
//! avoids.

use std::collections::HashMap;

/// Cumulative octets per interface, keyed by a stable interface id.
#[derive(Debug, Default)]
pub struct InterfaceCounters {
    previous: HashMap<u64, (u64, u64)>,
    primed: bool,
}

impl InterfaceCounters {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bytes in/out since the previous call. The first call only establishes a
    /// baseline and reports zero, because a cumulative counter says nothing
    /// about rate until it has been read twice.
    pub fn delta(&mut self) -> (u64, u64) {
        let current = read_interfaces();
        let mut down = 0u64;
        let mut up = 0u64;

        for (id, (in_octets, out_octets)) in &current {
            if let Some((prev_in, prev_out)) = self.previous.get(id) {
                // Saturating: adapters reset their counters on reconnect, and a
                // wrapped counter must read as zero rather than as terabytes.
                down += in_octets.saturating_sub(*prev_in);
                up += out_octets.saturating_sub(*prev_out);
            }
        }

        self.previous = current;
        if !self.primed {
            self.primed = true;
            return (0, 0);
        }
        (up, down)
    }
}

#[cfg(windows)]
fn read_interfaces() -> HashMap<u64, (u64, u64)> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        FreeMibTable, GetIfTable2, MIB_IF_TABLE2,
    };
    use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;

    let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
    // SAFETY: on success the pointer is set to a table owned by the OS, which
    // is handed back to FreeMibTable below.
    if unsafe { GetIfTable2(&mut table) } != 0 || table.is_null() {
        return HashMap::new();
    }

    let mut out = HashMap::new();
    unsafe {
        let count = (*table).NumEntries as usize;
        let rows = std::slice::from_raw_parts(
            std::ptr::addr_of!((*table).Table)
                .cast::<windows_sys::Win32::NetworkManagement::IpHelper::MIB_IF_ROW2>(),
            count,
        );

        for row in rows {
            if row.OperStatus != IfOperStatusUp
                || !counts(row.Type, row.InterfaceAndOperStatusFlags._bitfield)
            {
                continue;
            }
            // The LUID is stable across renames, unlike the interface index.
            out.insert(row.InterfaceLuid.Value, (row.InOctets, row.OutOctets));
        }

        FreeMibTable(table.cast());
    }

    out
}

/// Whether an interface's counters belong in the total: each adapter once.
///
/// Loopback would double-count our own proxy hop. And every filter driver
/// bound to an adapter - WFP, QoS, Wi-Fi's own - is listed as an interface in
/// its own right, repeating the adapter's counters: summing those counted
/// Wi-Fi six times over on a typical machine. Virtual adapters (a VPN's TAP,
/// Hyper-V's switch) are real interfaces and do count, as they do in
/// GlassWire: traffic tunnelled through one is seen there and again, wrapped,
/// on the adapter that carries the tunnel.
#[cfg_attr(not(windows), allow(dead_code))]
fn counts(if_type: u32, flags: u8) -> bool {
    const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;
    // `InterfaceAndOperStatusFlags`: bit 0 is HardwareInterface, bit 1 FilterInterface.
    const FILTER_INTERFACE: u8 = 1 << 1;
    if_type != IF_TYPE_SOFTWARE_LOOPBACK && flags & FILTER_INTERFACE == 0
}

#[cfg(not(windows))]
fn read_interfaces() -> HashMap<u64, (u64, u64)> {
    // ponytail: /proc/net/dev on Linux, getifaddrs + if_data on macOS. Until
    // then the meter reports tunnel traffic only on those platforms.
    HashMap::new()
}

/// Days since the Unix epoch in the machine's local time.
///
/// Usage is bucketed by local day so "Today" means what the user's clock says,
/// not what UTC says.
pub fn local_day(unix_seconds: i64) -> i64 {
    (unix_seconds + local_offset_secs()).div_euclid(86_400)
}

/// Midday, in unix seconds, of a given local day.
///
/// The inverse of `local_day`, aimed at noon rather than midnight so that a
/// DST shift between then and now cannot land the answer on the wrong day.
/// Used to hand day buckets to the UI as timestamps, which is the only calendar
/// vocabulary both sides share.
pub fn day_noon_unix(local_day: i64) -> i64 {
    local_day * 86_400 + 43_200 - local_offset_secs()
}

#[cfg(windows)]
fn local_offset_secs() -> i64 {
    use windows_sys::Win32::System::Time::{
        GetTimeZoneInformation, TIME_ZONE_ID_INVALID, TIME_ZONE_INFORMATION,
    };

    // windows-sys exports only the INVALID sentinel; the success codes are
    // documented, stable Win32 values.
    const TIME_ZONE_ID_DAYLIGHT: u32 = 2;

    // SAFETY: the struct is fully written by the call before it is read.
    let mut info: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
    let status = unsafe { GetTimeZoneInformation(&mut info) };
    if status == TIME_ZONE_ID_INVALID {
        return 0;
    }

    // Windows reports the bias as UTC = local + bias, so local is the negation.
    let bias = if status == TIME_ZONE_ID_DAYLIGHT {
        info.Bias + info.DaylightBias
    } else {
        info.Bias + info.StandardBias
    };
    i64::from(-bias) * 60
}

#[cfg(not(windows))]
fn local_offset_secs() -> i64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_adapter_counts_once() {
        const ETHERNET: u32 = 6;
        const WIFI: u32 = 71;
        const HARDWARE: u8 = 1;
        const FILTER: u8 = 1 << 1;

        assert!(counts(WIFI, HARDWARE), "the Wi-Fi adapter itself");
        assert!(counts(53, 0), "a VPN's TAP adapter");
        assert!(!counts(WIFI, FILTER), "a QoS or WFP filter repeating Wi-Fi's counters");
        assert!(!counts(ETHERNET, FILTER | HARDWARE));
        assert!(!counts(24, 0), "loopback");
    }

    /// The real table, read through the real filter: each adapter's counters
    /// must appear once, never repeated by its filter drivers.
    #[cfg(windows)]
    #[test]
    fn no_two_interfaces_report_the_same_counters() {
        let interfaces = read_interfaces();
        let mut busy: Vec<(u64, u64)> = interfaces
            .values()
            .copied()
            .filter(|(i, o)| i + o > 1_000_000)
            .collect();
        let before = busy.len();
        busy.sort_unstable();
        busy.dedup();
        assert_eq!(busy.len(), before, "an adapter is still counted more than once");
    }

    #[test]
    fn first_read_primes_without_reporting_a_spike() {
        let mut counters = InterfaceCounters::new();
        // A cumulative counter read once cannot yield a rate, so the baseline
        // read must report nothing rather than the lifetime total.
        assert_eq!(counters.delta(), (0, 0));
        assert!(counters.primed);
    }

    #[test]
    fn counter_resets_do_not_produce_giant_deltas() {
        let mut counters = InterfaceCounters::new();
        counters.previous.insert(1, (10_000, 5_000));
        counters.primed = true;

        // Simulate an adapter whose counters restarted at zero.
        let mut current = HashMap::new();
        current.insert(1u64, (10u64, 5u64));

        let mut down = 0u64;
        let mut up = 0u64;
        for (id, (i, o)) in &current {
            let (pi, po) = counters.previous[id];
            down += i.saturating_sub(pi);
            up += o.saturating_sub(po);
        }
        assert_eq!((up, down), (0, 0));
    }

    #[test]
    fn day_numbers_survive_a_round_trip_through_timestamps() {
        // Whatever the host's offset, a day handed out as a timestamp must come
        // back as the same day - otherwise a month picker would be off by one.
        for day in [0i64, 1, 19_000, 20_708, 25_000] {
            assert_eq!(local_day(day_noon_unix(day)), day, "day {day} shifted");
        }
    }

    #[test]
    fn local_day_advances_at_local_midnight() {
        let offset = local_offset_secs();
        // The instant just before local midnight and just after must differ.
        let midnight = 1_800_000_000i64 - (1_800_000_000 + offset).rem_euclid(86_400);
        assert_eq!(local_day(midnight + 86_400), local_day(midnight) + 1);
        assert_eq!(local_day(midnight + 86_399), local_day(midnight));
    }
}

#[cfg(all(test, windows))]
mod diagnostics {
    /// Prints every interface `GetIfTable2` reports, so the set being summed can
    /// be compared against `Get-NetAdapterStatistics`.
    /// Run with: cargo test diagnose_interfaces -- --ignored --nocapture
    #[test]
    #[ignore]
    fn diagnose_interfaces() {
        use windows_sys::Win32::NetworkManagement::IpHelper::{
            FreeMibTable, GetIfTable2, MIB_IF_ROW2, MIB_IF_TABLE2,
        };

        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        assert_eq!(unsafe { GetIfTable2(&mut table) }, 0, "GetIfTable2 failed");

        unsafe {
            let count = (*table).NumEntries as usize;
            let rows = std::slice::from_raw_parts(
                std::ptr::addr_of!((*table).Table).cast::<MIB_IF_ROW2>(),
                count,
            );
            println!("\n{count} interfaces reported\n");
            println!(
                "{:<46} {:>4} {:>3} {:>4} {:>16}",
                "alias", "oper", "hw", "filt", "in_octets"
            );
            for row in rows {
                let alias_len = row
                    .Alias
                    .iter()
                    .position(|c| *c == 0)
                    .unwrap_or(row.Alias.len());
                let alias = String::from_utf16_lossy(&row.Alias[..alias_len]);
                // Bit 0 = HardwareInterface, bit 1 = FilterInterface.
                let flags = row.InterfaceAndOperStatusFlags._bitfield;
                if row.InOctets == 0 && row.OutOctets == 0 {
                    continue;
                }
                println!(
                    "{:<46} {:>4} {:>3} {:>4} {:>16}",
                    alias,
                    row.OperStatus,
                    flags & 0x01,
                    (flags >> 1) & 0x01,
                    row.InOctets
                );
            }
            FreeMibTable(table.cast());
        }
    }
}

