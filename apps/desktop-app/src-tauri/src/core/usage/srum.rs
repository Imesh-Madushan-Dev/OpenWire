//! Spike: device-wide per-app network usage from Windows' own accounting.
//!
//! Windows records per-app bytes for the Settings -> Network -> Data usage page
//! in SRUM, and surfaces it through WinRT's `GetAttributedNetworkUsageAsync`.
//! If that works for an unpackaged desktop app, it is per-app attribution for
//! *all* traffic with no driver, no ETW session and no elevation - which is
//! everything the relay cannot see.
//!
//! Run with: cargo test srum -- --ignored --nocapture

#[cfg(windows)]
#[allow(dead_code)]
pub fn probe(from_unix: i64, to_unix: i64) -> windows_core::Result<Vec<(String, u64, u64)>> {
    use windows::Foundation::DateTime;
    use windows::Networking::Connectivity::{NetworkInformation, NetworkUsageStates, TriStates};

    // WinRT ticks: 100ns units since 1601-01-01.
    const EPOCH_DELTA: i64 = 11_644_473_600;
    let to_ticks = |unix: i64| DateTime {
        UniversalTime: (unix + EPOCH_DELTA) * 10_000_000,
    };

    let states = NetworkUsageStates {
        Roaming: TriStates::DoNotCare,
        Shared: TriStates::DoNotCare,
    };

    let mut out = Vec::new();
    for profile in NetworkInformation::GetConnectionProfiles()? {
        let usage = profile
            .GetAttributedNetworkUsageAsync(to_ticks(from_unix), to_ticks(to_unix), states)?
            .get()?;

        for entry in usage {
            out.push((
                entry.AttributionName()?.to_string_lossy(),
                entry.BytesSent()?,
                entry.BytesReceived()?,
            ));
        }
    }

    Ok(out)
}

#[cfg(all(test, windows))]
mod tests {
    /// Does Windows hand an unpackaged desktop app its per-app usage table?
    #[test]
    #[ignore]
    fn srum_reports_per_app_usage_for_the_last_week() {
        use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

        // SAFETY: initialising the apartment for this test thread only.
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        match super::probe(now - 7 * 86_400, now) {
            Ok(rows) => {
                println!("\n{} attributed rows\n", rows.len());
                let mut rows = rows;
                rows.sort_by_key(|(_, up, down)| std::cmp::Reverse(up + down));
                for (name, up, down) in rows.iter().take(25) {
                    println!("{:>12} up {:>12} down  {name}", up, down);
                }
                let total: u64 = rows.iter().map(|(_, up, down)| up + down).sum();
                println!("\ntotal {total} bytes across {} rows", rows.len());
                assert!(!rows.is_empty(), "no rows: SRUM route is not usable here");
            }
            Err(err) => panic!("GetAttributedNetworkUsageAsync failed: {err}"),
        }
    }
}
