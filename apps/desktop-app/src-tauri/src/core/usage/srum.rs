//! Device-wide per-app usage, from Windows' own accounting.
//!
//! Windows records per-app network bytes for the Settings -> Network -> Data
//! usage page, and surfaces it through WinRT's `GetAttributedNetworkUsageAsync`.
//! Unlike the relay, this sees **every** app on the machine whether or not it
//! honours the system proxy - and unlike ETW or a WFP driver it needs no
//! elevation, because Windows already did the collecting.
//!
//! What it does not carry is destinations: there is no host or port here, only
//! bytes per app. That is why the Hosts and Traffic-type columns stay relay-fed
//! while the Apps column can cover the whole device.
//!
//! Cost: a machine accumulates a `ConnectionProfile` per network it has ever
//! joined (42 on the development box) and each is a separate round-trip, so a
//! full query takes seconds even with every request in flight at once. Callers
//! must treat this as a report to fetch and cache, never as a poll.

/// One app's bytes over the queried period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppBytes {
    /// Display name: the executable's file name, or a readable package name.
    pub name: String,
    /// Full path, when the attribution was a plain executable. Feeds icons.
    pub path: Option<String>,
    pub up: u64,
    pub down: u64,
}

/// Bytes Windows could not attribute to a user-mode app: kernel, drivers and
/// the shared service hosts. Labelled rather than hidden, so the column's
/// figures can be reconciled against the ring.
pub const SYSTEM_KEY: &str = "System and services";

#[cfg(not(windows))]
pub fn per_app(_from_unix: i64, _to_unix: i64) -> Option<Vec<AppBytes>> {
    // ponytail: nothing equivalent on macOS or Linux without a privileged
    // helper, which is the same reason the system counters are Windows-only.
    None
}

#[cfg(windows)]
pub fn per_app(from_unix: i64, to_unix: i64) -> Option<Vec<AppBytes>> {
    use std::collections::HashMap;

    let rows = query(from_unix, to_unix).ok()?;
    let volumes = volume_map();

    // Merged by display name: the same executable appears once per connection
    // profile, and versioned installs (a tool under .../ext-1.2.3/bin) would
    // otherwise each get a row of their own.
    let mut merged: HashMap<String, AppBytes> = HashMap::new();
    for (id, up, down) in rows {
        let (name, path) = identify(&id, &volumes);
        let entry = merged.entry(name.clone()).or_insert(AppBytes {
            name,
            path,
            up: 0,
            down: 0,
        });
        entry.up += up;
        entry.down += down;
    }

    let mut out: Vec<AppBytes> = merged.into_values().collect();
    out.sort_unstable_by(|a, b| {
        (b.up + b.down)
            .cmp(&(a.up + a.down))
            .then_with(|| a.name.cmp(&b.name))
    });
    Some(out)
}

/// `(attribution id, bytes up, bytes down)` straight from WinRT.
#[cfg(windows)]
fn query(from_unix: i64, to_unix: i64) -> windows_core::Result<Vec<(String, u64, u64)>> {
    use windows::Foundation::DateTime;
    use windows::Networking::Connectivity::{NetworkInformation, NetworkUsageStates, TriStates};

    ensure_mta();

    // WinRT ticks: 100ns units since 1601-01-01.
    const EPOCH_DELTA: i64 = 11_644_473_600;
    let to_ticks = |unix: i64| DateTime {
        UniversalTime: (unix + EPOCH_DELTA) * 10_000_000,
    };

    let states = NetworkUsageStates {
        Roaming: TriStates::DoNotCare,
        Shared: TriStates::DoNotCare,
    };

    // Every query is started before any is awaited. Serialising them costs
    // ~350ms per profile, which on a well-travelled laptop is 15 seconds.
    let mut pending = Vec::new();
    for profile in NetworkInformation::GetConnectionProfiles()? {
        if let Ok(op) =
            profile.GetAttributedNetworkUsageAsync(to_ticks(from_unix), to_ticks(to_unix), states)
        {
            pending.push(op);
        }
    }

    let mut out = Vec::new();
    for op in pending {
        // One failed profile must not lose the other forty.
        let Ok(usage) = op.get() else { continue };

        for entry in usage {
            let (Ok(id), Ok(up), Ok(down)) = (
                entry.AttributionId(),
                entry.BytesSent(),
                entry.BytesReceived(),
            ) else {
                continue;
            };
            if up == 0 && down == 0 {
                continue;
            }
            out.push((id.to_string_lossy(), up, down));
        }
    }

    Ok(out)
}

/// WinRT needs an initialised apartment on the calling thread. An MTA held for
/// the life of the process covers every worker thread the query may land on,
/// without re-initialising per call.
#[cfg(windows)]
fn ensure_mta() {
    use std::sync::OnceLock;
    static MTA: OnceLock<()> = OnceLock::new();

    MTA.get_or_init(|| {
        let mut cookie = std::ptr::null_mut();
        // SAFETY: the cookie is deliberately never released - the apartment is
        // meant to outlive every call, for the life of the process.
        unsafe { windows_sys::Win32::System::Com::CoIncrementMTAUsage(&mut cookie) };
    });
}

/// Turns an attribution id into something a person recognises.
///
/// Ids arrive in two shapes: an NT device path for a plain executable
/// (`\device\harddiskvolume9\...\msedge.exe`), or a package family name for a
/// Store app (`5319275A.WhatsAppDesktop_cv1g1gvanyjgm`).
#[cfg(windows)]
fn identify(id: &str, volumes: &[(String, String)]) -> (String, Option<String>) {
    if id.is_empty() {
        return (SYSTEM_KEY.to_string(), None);
    }

    let lower = id.to_ascii_lowercase();
    if !lower.starts_with("\\device\\") {
        return (package_name(id), None);
    }

    let name = id
        .rsplit('\\')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(id)
        .to_string();

    // A DOS path is what the icon extractor can actually open.
    let path = volumes.iter().find_map(|(device, drive)| {
        lower
            .strip_prefix(device.as_str())
            .map(|rest| format!("{drive}{rest}"))
    });

    (name, path)
}

/// `5319275A.WhatsAppDesktop_cv1g1gvanyjgm` -> `WhatsAppDesktop`.
///
/// Derived rather than looked up: resolving the real display name needs
/// `PackageManager`, which wants a capability an unpackaged app does not have.
#[cfg(windows)]
fn package_name(family: &str) -> String {
    let without_hash = family.rsplit_once('_').map_or(family, |(head, _)| head);
    let leaf = without_hash
        .rsplit_once('.')
        .map_or(without_hash, |(_, leaf)| leaf);

    if leaf.is_empty() {
        family.to_string()
    } else {
        leaf.to_string()
    }
}

/// `\device\harddiskvolume9` -> `C:`, for every drive letter currently mapped.
#[cfg(windows)]
fn volume_map() -> Vec<(String, String)> {
    use windows_sys::Win32::Storage::FileSystem::QueryDosDeviceW;

    let mut out = Vec::new();
    let mut buffer = [0u16; 512];

    for letter in b'A'..=b'Z' {
        let drive = format!("{}:", letter as char);
        let wide: Vec<u16> = drive.encode_utf16().chain(std::iter::once(0)).collect();

        // SAFETY: a fixed buffer whose length is passed in characters; a
        // failure returns 0 and writes nothing.
        let len =
            unsafe { QueryDosDeviceW(wide.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32) };
        if len == 0 {
            continue;
        }

        // The result is double-NUL terminated; the first string is the target.
        let end = buffer.iter().position(|c| *c == 0).unwrap_or(0);
        if end == 0 {
            continue;
        }

        out.push((
            String::from_utf16_lossy(&buffer[..end]).to_ascii_lowercase(),
            drive,
        ));
    }

    out
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn an_nt_path_yields_the_exe_name_and_a_dos_path() {
        let volumes = vec![("\\device\\harddiskvolume9".to_string(), "C:".to_string())];
        let (name, path) = identify(
            "\\device\\harddiskvolume9\\program files\\google\\chrome\\application\\chrome.exe",
            &volumes,
        );

        assert_eq!(name, "chrome.exe");
        assert_eq!(
            path.as_deref(),
            Some("C:\\program files\\google\\chrome\\application\\chrome.exe")
        );
    }

    #[test]
    fn an_unmapped_volume_still_names_the_app() {
        // A path on a volume with no drive letter loses its icon, not its row.
        let (name, path) = identify("\\device\\harddiskvolume77\\tool.exe", &[]);
        assert_eq!(name, "tool.exe");
        assert_eq!(path, None);
    }

    #[test]
    fn a_package_family_name_is_trimmed_to_something_readable() {
        assert_eq!(
            package_name("5319275A.WhatsAppDesktop_cv1g1gvanyjgm"),
            "WhatsAppDesktop"
        );
        assert_eq!(
            package_name("SpotifyAB.SpotifyMusic_zpdnekdrzrea0"),
            "SpotifyMusic"
        );
        assert_eq!(
            package_name("MicrosoftWindows.CrossDevice_cw5n1h2txyewy"),
            "CrossDevice"
        );
        // Nothing recognisable to trim: keep the id rather than return empty.
        assert_eq!(package_name("Weird_"), "Weird");
    }

    #[test]
    fn unattributed_bytes_are_labelled_rather_than_blank() {
        let (name, path) = identify("", &[]);
        assert_eq!(name, SYSTEM_KEY);
        assert_eq!(path, None);
    }

    /// Reads the machine's real usage table. Ignored: it depends on the host
    /// having history, and takes seconds.
    /// Run with: cargo test srum_reports -- --ignored --nocapture
    #[test]
    #[ignore]
    fn srum_reports_real_per_app_usage() {
        let now = crate::core::usage::unix_seconds();
        let started = std::time::Instant::now();
        let rows = per_app(now - 30 * 86_400, now).expect("SRUM should answer on Windows");
        println!("\n{} apps in {:?}\n", rows.len(), started.elapsed());

        for row in rows.iter().take(20) {
            println!(
                "{:>14} {:>14}  {:<28} {}",
                row.up,
                row.down,
                row.name,
                row.path.as_deref().unwrap_or("-")
            );
        }

        assert!(!rows.is_empty());
        assert!(
            rows.iter().any(|row| row.path.is_some()),
            "at least one row should resolve to a real path for its icon"
        );
    }
}
