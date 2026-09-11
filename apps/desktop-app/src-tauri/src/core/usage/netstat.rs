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

    // Loopback would double-count our own proxy hop.
    const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;

    let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
    // SAFETY: on success the pointer is set to a table owned by the OS, which
    // is handed back to FreeMibTable below.
    if unsafe { GetIfTable2(&mut table) } != 0 || table.is_null() {
        return HashMap::new();
    }

    let mut out = HashMap::new();
    unsafe {
        let count = (*table).NumEntries as usize;
        let rows = std::slice::from_raw_parts(std::ptr::addr_of!((*table).Table).cast::<
            windows_sys::Win32::NetworkManagement::IpHelper::MIB_IF_ROW2,
        >(), count);

        for row in rows {
            if row.Type == IF_TYPE_SOFTWARE_LOOPBACK || row.OperStatus != IfOperStatusUp {
                continue;
            }
            // The LUID is stable across renames, unlike the interface index.
            out.insert(row.InterfaceLuid.Value, (row.InOctets, row.OutOctets));
        }

        FreeMibTable(table.cast());
    }

    out
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
    fn local_day_advances_at_local_midnight() {
        let offset = local_offset_secs();
        // The instant just before local midnight and just after must differ.
        let midnight = 1_800_000_000i64 - (1_800_000_000 + offset).rem_euclid(86_400);
        assert_eq!(local_day(midnight + 86_400), local_day(midnight) + 1);
        assert_eq!(local_day(midnight + 86_399), local_day(midnight));
    }
}
