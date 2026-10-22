//! The connection log behind the server list's Logs button: each step of
//! bringing a tunnel up, keeping it up and taking it down, in plain words -
//! the running commentary NetMod and v2rayN users expect when something
//! will not connect.
//!
//! Memory only, and capped. A record of which servers you used and when has
//! no business being written to disk.

use std::collections::VecDeque;
use std::sync::Mutex;

use serde::Serialize;

/// Many sessions' worth; the oldest lines fall off the top.
const CAPACITY: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    /// Increasing, so the UI can merge a fetched list with live events.
    pub id: u64,
    /// Unix milliseconds.
    pub ts: u64,
    pub level: Level,
    pub message: String,
    /// The first line of a connection attempt, which the UI sets apart.
    pub session_start: bool,
}

#[derive(Default)]
pub struct ConnLog {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    next_id: u64,
    entries: VecDeque<LogEntry>,
}

impl ConnLog {
    pub fn push(&self, ts: u64, level: Level, message: String, session_start: bool) -> LogEntry {
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.next_id += 1;
        let entry = LogEntry {
            id: inner.next_id,
            ts,
            level,
            message,
            session_start,
        };
        if inner.entries.len() == CAPACITY {
            inner.entries.pop_front();
        }
        inner.entries.push_back(entry.clone());
        entry
    }

    pub fn entries(&self) -> Vec<LogEntry> {
        let inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.entries.iter().cloned().collect()
    }

    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.entries.clear();
    }
}

/// The OS the app runs on, for the first line of a session, e.g.
/// "Windows 11 Pro 25H2 (build 26200), 64-bit".
pub fn system_summary() -> String {
    let arch = if cfg!(target_pointer_width = "64") { "64-bit" } else { "32-bit" };
    format!("{}, {arch}", os_name())
}

#[cfg(windows)]
fn os_name() -> String {
    use winreg::enums::HKEY_LOCAL_MACHINE;

    let Ok(key) = winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
    else {
        return "Windows".to_string();
    };
    let product: String = key.get_value("ProductName").unwrap_or_else(|_| "Windows".into());
    let build: String = key.get_value("CurrentBuild").unwrap_or_default();
    let release: String = key.get_value("DisplayVersion").unwrap_or_default();

    let mut name = windows_product(&product, &build);
    if !release.is_empty() {
        name.push(' ');
        name.push_str(&release);
    }
    if !build.is_empty() {
        name.push_str(&format!(" (build {build})"));
    }
    name
}

#[cfg(not(windows))]
fn os_name() -> String {
    std::env::consts::OS.to_string()
}

/// Windows 11 still calls itself "Windows 10" in the registry; its build
/// number is what tells them apart.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_product(product: &str, build: &str) -> String {
    let eleven = build.parse::<u32>().is_ok_and(|build| build >= 22_000);
    if eleven {
        product.replacen("Windows 10", "Windows 11", 1)
    } else {
        product.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_newest_lines_once_full() {
        let log = ConnLog::default();
        for i in 0..CAPACITY + 3 {
            log.push(i as u64, Level::Info, format!("line {i}"), false);
        }
        let entries = log.entries();
        assert_eq!(entries.len(), CAPACITY);
        assert_eq!(entries[0].message, "line 3");
        assert_eq!(entries.last().unwrap().id, (CAPACITY + 3) as u64);
    }

    #[test]
    fn ids_keep_counting_after_a_clear() {
        let log = ConnLog::default();
        log.push(0, Level::Info, "a".into(), true);
        log.clear();
        assert!(log.entries().is_empty());
        assert_eq!(log.push(0, Level::Info, "b".into(), false).id, 2);
    }

    #[test]
    fn windows_11_is_named_by_its_build() {
        assert_eq!(windows_product("Windows 10 Pro", "26200"), "Windows 11 Pro");
        assert_eq!(windows_product("Windows 10 Home", "19045"), "Windows 10 Home");
        assert_eq!(windows_product("Windows 10 Pro", ""), "Windows 10 Pro");
    }
}
