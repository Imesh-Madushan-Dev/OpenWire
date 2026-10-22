//! Small Win32 helpers shared by the service, its installer and the client.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// NUL-terminated UTF-16, for every `PCWSTR` parameter.
pub fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Every unicast address assigned to this machine, for telling the local end
/// of a flow from the remote one.
pub fn local_addresses() -> HashSet<IpAddr> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        FreeMibTable, GetUnicastIpAddressTable, MIB_UNICASTIPADDRESS_ROW,
        MIB_UNICASTIPADDRESS_TABLE,
    };
    use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6, AF_UNSPEC};

    let mut table: *mut MIB_UNICASTIPADDRESS_TABLE = std::ptr::null_mut();
    // SAFETY: on success the OS hands back a table it owns, released below.
    if unsafe { GetUnicastIpAddressTable(AF_UNSPEC, &mut table) } != 0 || table.is_null() {
        return HashSet::new();
    }

    let mut out = HashSet::new();
    // SAFETY: `NumEntries` rows follow the header contiguously, and each row's
    // address union is tagged by `si_family`.
    unsafe {
        let rows = std::slice::from_raw_parts(
            std::ptr::addr_of!((*table).Table).cast::<MIB_UNICASTIPADDRESS_ROW>(),
            (*table).NumEntries as usize,
        );
        for row in rows {
            match row.Address.si_family {
                AF_INET => {
                    // Network byte order as stored, so the in-memory bytes are
                    // already the address's octets.
                    let raw = row.Address.Ipv4.sin_addr.S_un.S_addr;
                    out.insert(IpAddr::V4(Ipv4Addr::from(raw.to_ne_bytes())));
                }
                AF_INET6 => {
                    let octets = row.Address.Ipv6.sin6_addr.u.Byte;
                    out.insert(IpAddr::V6(Ipv6Addr::from(octets)).to_canonical());
                }
                _ => {}
            }
        }
        FreeMibTable(table.cast());
    }
    out
}

/// `C:\Program Files`, read from the known-folder registry rather than the
/// environment: the elevated installer must not be steerable into a folder
/// the user can write to by a doctored `%ProgramFiles%`.
pub fn program_files() -> Option<PathBuf> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramFiles, SHGetKnownFolderPath};

    let mut raw: windows_sys::core::PWSTR = std::ptr::null_mut();
    // SAFETY: the out-pointer is freed with CoTaskMemFree whatever the result,
    // as the API requires.
    let hr =
        unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramFiles, 0, std::ptr::null_mut(), &mut raw) };
    if raw.is_null() {
        return None;
    }

    let path = if hr >= 0 {
        // SAFETY: on success `raw` is a NUL-terminated string.
        let len = (0..).take_while(|&i| unsafe { *raw.add(i) } != 0).count();
        Some(PathBuf::from(String::from_utf16_lossy(unsafe {
            std::slice::from_raw_parts(raw, len)
        })))
    } else {
        None
    };
    unsafe { CoTaskMemFree(raw.cast()) };
    path
}

/// Where the service's copy of the binary lives.
pub fn service_dir() -> Option<PathBuf> {
    program_files().map(|root| root.join("OpenWire Network Monitor"))
}

pub fn service_exe() -> Option<PathBuf> {
    service_dir().map(|dir| dir.join("openwire-netmon.exe"))
}

// ---------------------------------------------------------------------------
// Service log
// ---------------------------------------------------------------------------

/// The service has no console and no Tauri logger, so problems go to a small
/// file beside its binary. Program Files and not ProgramData on purpose: a
/// user can pre-create paths under ProgramData, and a SYSTEM process writing
/// wherever a user pointed it is a classic way to overwrite protected files.
struct FileLog;

static LOG: OnceLock<Mutex<File>> = OnceLock::new();
const LOG_LIMIT: u64 = 512 * 1024;

impl log::Log for FileLog {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let Some(file) = LOG.get() else { return };
        let Ok(mut file) = file.lock() else { return };

        // Truncated rather than rotated: this is for diagnosing the current
        // problem, not for keeping history.
        if file
            .metadata()
            .map(|meta| meta.len() > LOG_LIMIT)
            .unwrap_or(false)
        {
            let _ = file.set_len(0);
        }
        let _ = writeln!(
            file,
            "{} {:<5} {}",
            crate::core::usage::unix_seconds(),
            record.level(),
            record.args()
        );
    }

    fn flush(&self) {}
}

static LOGGER: FileLog = FileLog;

pub fn init_service_log() {
    let Some(path) = service_dir().map(|dir| dir.join("netmon.log")) else {
        return;
    };
    let Ok(file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    if LOG.set(Mutex::new(file)).is_ok() && log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_machine_has_addresses_and_a_program_files() {
        let local = local_addresses();
        assert!(
            local.iter().any(|ip| ip.is_loopback()),
            "loopback is always assigned: {local:?}"
        );

        let exe = service_exe().expect("known folder resolves");
        assert!(exe.starts_with(program_files().unwrap()));
        assert!(exe.ends_with("OpenWire Network Monitor/openwire-netmon.exe"));
    }

    #[test]
    fn querying_a_missing_service_is_quiet() {
        // Not installed on a build machine; and when it is, the answer still
        // has to be self-consistent.
        let state = super::super::scm::query();
        assert!(state.installed || !state.running);
    }
}
