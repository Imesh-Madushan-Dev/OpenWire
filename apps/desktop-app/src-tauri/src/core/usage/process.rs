//! Per-app attribution.
//!
//! Apps reach the tunnel over loopback, so the connection's client-side port is
//! enough to identify the owning process: look the port up in the OS TCP table,
//! then resolve the PID to an executable name. No kernel driver, no elevation -
//! which is why this works at all without the privileges GlassWire needs.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::icon;

pub const UNKNOWN_APP: &str = "Unknown app";

/// Caches the TCP table and PID lookups, because a new connection would
/// otherwise mean a syscall plus a process open on the accept path.
pub struct ProcessResolver {
    inner: Mutex<Inner>,
}

struct Inner {
    /// (local port, pid) pairs from the last table read.
    table: Vec<(u16, u32)>,
    read_at: Option<Instant>,
    names: HashMap<u32, String>,
    /// Executable name -> full path, so an icon can be extracted later without
    /// the process still being alive.
    paths: HashMap<String, String>,
    /// Extracted icons, `None` for an exe we already failed on - the summary is
    /// polled every ten seconds and must not retry the shell each time.
    icons: HashMap<String, Option<String>>,
}

/// A table older than this is refreshed before reporting a miss; a connection
/// that was just accepted may be newer than the snapshot we hold.
const STALE_AFTER: Duration = Duration::from_millis(250);

impl Default for ProcessResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessResolver {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                table: Vec::new(),
                read_at: None,
                names: HashMap::new(),
                paths: HashMap::new(),
                icons: HashMap::new(),
            }),
        }
    }

    /// Executable name owning `local_port`, or `None` when it cannot be found.
    pub fn app_for_port(&self, local_port: u16) -> Option<String> {
        let mut inner = self.inner.lock().ok()?;

        let fresh = inner.read_at.is_some_and(|at| at.elapsed() < STALE_AFTER);

        let mut pid = lookup(&inner.table, local_port);

        // One retry against a fresh table: the miss is usually just a snapshot
        // taken before this connection existed.
        if pid.is_none() || !fresh {
            if let Some(table) = read_tcp_table() {
                inner.table = table;
                inner.read_at = Some(Instant::now());
                pid = lookup(&inner.table, local_port);
            }
        }

        let pid = pid?;
        if let Some(name) = inner.names.get(&pid) {
            return Some(name.clone());
        }

        let (name, path) = process_image(pid)?;
        inner.names.insert(pid, name.clone());
        inner.paths.insert(name.clone(), path);
        // PIDs are recycled, so this cache must not grow forever.
        if inner.names.len() > 512 {
            inner.names.clear();
        }
        Some(name)
    }

    /// PNG data URL for an app's icon, or `None` when its path is unknown or
    /// carries no icon.
    pub fn icon_for_app(&self, app: &str) -> Option<String> {
        let path = {
            let inner = self.inner.lock().ok()?;
            if let Some(cached) = inner.icons.get(app) {
                return cached.clone();
            }
            inner.paths.get(app)?.clone()
        };

        // Read outside the lock: it touches the disk, and the accept path
        // takes this lock for every new connection.
        let extracted = icon::data_url(&path);
        if let Ok(mut inner) = self.inner.lock() {
            inner.icons.insert(app.to_string(), extracted.clone());
        }
        extracted
    }

    /// The name -> path pairs learned so far, for persisting alongside history.
    pub fn known_paths(&self) -> HashMap<String, String> {
        self.inner
            .lock()
            .map(|inner| inner.paths.clone())
            .unwrap_or_default()
    }

    /// Restores paths from a previous run so icons appear before the app in
    /// question has opened a connection in this one. Never replaces a path
    /// already known: a restored or historical one is older than any seen live.
    pub fn seed_paths(&self, paths: HashMap<String, String>) {
        if let Ok(mut inner) = self.inner.lock() {
            for (name, path) in paths {
                inner.paths.entry(name).or_insert(path);
            }
        }
    }

    /// Records where running apps live right now. Unlike [`Self::seed_paths`]
    /// this replaces what was known: an update moves an app (Edge's WebView
    /// and Store apps keep a folder per version), and the old path then names
    /// a file that is gone.
    pub fn learn_paths(&self, paths: HashMap<String, String>) {
        if let Ok(mut inner) = self.inner.lock() {
            for (name, path) in paths {
                if inner.paths.get(&name) != Some(&path) {
                    // Drawn from the old file, if at all.
                    inner.icons.remove(&name);
                    inner.paths.insert(name, path);
                }
            }
        }
    }
}

fn lookup(table: &[(u16, u32)], port: u16) -> Option<u32> {
    table
        .iter()
        .find(|(entry_port, _)| *entry_port == port)
        .map(|(_, pid)| *pid)
}

// ---------------------------------------------------------------------------

#[cfg(windows)]
fn read_tcp_table() -> Option<Vec<(u16, u32)>> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
    };
    use windows_sys::Win32::Networking::WinSock::AF_INET;

    let mut size: u32 = 0;
    // First call sizes the buffer; the error return is expected here.
    unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        );
    }
    if size == 0 {
        return None;
    }

    // u32-aligned so the MIB structs inside are correctly aligned.
    let mut buffer: Vec<u32> = vec![0; (size as usize).div_ceil(4)];
    let result = unsafe {
        GetExtendedTcpTable(
            buffer.as_mut_ptr().cast(),
            &mut size,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        )
    };
    if result != 0 {
        return None;
    }

    // SAFETY: the call succeeded, so the buffer holds a MIB_TCPTABLE_OWNER_PID
    // whose `dwNumEntries` rows follow the header contiguously.
    let table = unsafe { &*(buffer.as_ptr() as *const MIB_TCPTABLE_OWNER_PID) };
    let count = table.dwNumEntries as usize;
    let rows = unsafe {
        std::slice::from_raw_parts(
            std::ptr::addr_of!(table.table) as *const MIB_TCPROW_OWNER_PID,
            count,
        )
    };

    Some(
        rows.iter()
            .map(|row| {
                // Ports sit in the low two bytes in network byte order.
                let port = u16::from_be_bytes([
                    (row.dwLocalPort & 0xff) as u8,
                    ((row.dwLocalPort >> 8) & 0xff) as u8,
                ]);
                (port, row.dwOwningPid)
            })
            .collect(),
    )
}

/// `(executable name, full path)` for a pid.
#[cfg(windows)]
pub(crate) fn process_image(pid: u32) -> Option<(String, String)> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: a failed open returns null, which is checked before use.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }

    let mut buffer = [0u16; 260];
    let mut len = buffer.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut len)
    };
    unsafe { CloseHandle(handle) };

    if ok == 0 || len == 0 {
        return None;
    }

    let full = String::from_utf16_lossy(&buffer[..len as usize]);
    // Only the file name is ever shown: the full path is noise in a usage list,
    // and hiding it keeps user directory names off screen. The path is still
    // kept, because extracting the icon needs it.
    let name = full
        .rsplit(['\\', '/'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(&full)
        .to_string();
    Some((name, full))
}

#[cfg(not(windows))]
fn read_tcp_table() -> Option<Vec<(u16, u32)>> {
    // ponytail: /proc/net/tcp on Linux and libproc on macOS both work the same
    // way; wire them up when those platforms get system-proxy support too.
    None
}

#[cfg(not(windows))]
fn process_image(_pid: u32) -> Option<(String, String)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_seen_live_replaces_a_restored_one_but_not_the_reverse() {
        let path = |p: &str| HashMap::from([("app.exe".to_string(), p.to_string())]);
        let resolver = ProcessResolver::new();

        resolver.seed_paths(path(r"C:\App\1.0\app.exe"));
        resolver.learn_paths(path(r"C:\App\2.0\app.exe"));
        resolver.seed_paths(path(r"C:\App\1.0\app.exe"));

        assert_eq!(
            resolver.known_paths()["app.exe"],
            r"C:\App\2.0\app.exe",
            "the app moved when it updated; the old folder is gone"
        );
    }

    #[test]
    fn lookup_matches_the_exact_port() {
        let table = vec![(1000u16, 4u32), (1001, 8)];
        assert_eq!(lookup(&table, 1001), Some(8));
        assert_eq!(lookup(&table, 1002), None);
    }

    /// Resolves this test process through the real OS table: bind a loopback
    /// socket, connect to it, then ask who owns the client port.
    #[cfg(windows)]
    #[test]
    fn resolves_a_real_loopback_connection_to_this_process() {
        use std::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let client = TcpStream::connect(addr).expect("connect");
        let _accepted = listener.accept().expect("accept");

        let port = client.local_addr().expect("local addr").port();
        let name = ProcessResolver::new()
            .app_for_port(port)
            .expect("the owning process should be found");

        assert!(
            name.ends_with(".exe"),
            "expected an executable name, got {name}"
        );
    }

    /// The resolver must remember where an app lives, since the icon is read
    /// from the file long after the connection closed.
    #[cfg(windows)]
    #[test]
    fn resolving_an_app_records_a_path_an_icon_can_be_read_from() {
        use std::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let client = TcpStream::connect(listener.local_addr().expect("addr")).expect("connect");
        let _accepted = listener.accept().expect("accept");

        let resolver = ProcessResolver::new();
        let name = resolver
            .app_for_port(client.local_addr().expect("local addr").port())
            .expect("owning process");

        let path = resolver.known_paths().get(&name).cloned().expect("a path");
        assert!(path.ends_with(&name), "{path} should end with {name}");
        // Whether *this* binary carries an icon is incidental - the test runner
        // has none. What matters is that the lookup runs off the stored path
        // and caches its answer; `icon::tests` covers real extraction.
        assert_eq!(resolver.icon_for_app(&name), resolver.icon_for_app(&name));
        assert_eq!(resolver.icon_for_app("never-seen.exe"), None);
    }
}
