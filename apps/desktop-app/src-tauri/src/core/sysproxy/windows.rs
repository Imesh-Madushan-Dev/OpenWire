//! Windows system proxy via the per-user WinINET settings.
//!
//! Everything here writes to HKCU only - no elevation, no machine-wide change,
//! and nothing another user's session can observe.

use std::ffi::c_void;

use serde::{Deserialize, Serialize};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

use crate::error::{AppError, AppResult};

pub const IS_SUPPORTED: bool = true;

const INTERNET_SETTINGS: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

const INTERNET_OPTION_SETTINGS_CHANGED: u32 = 39;
const INTERNET_OPTION_REFRESH: u32 = 37;

#[link(name = "wininet")]
extern "system" {
    fn InternetSetOptionW(
        h_internet: *mut c_void,
        dw_option: u32,
        lp_buffer: *mut c_void,
        dw_buffer_length: u32,
    ) -> i32;
}

/// Exactly the three values we overwrite, so restore is a faithful rollback
/// rather than a guess at sane defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySnapshot {
    enable: u32,
    server: Option<String>,
    override_list: Option<String>,
}

fn open(access: u32) -> AppResult<RegKey> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(INTERNET_SETTINGS, access)
        .map_err(|e| AppError::SystemProxy(format!("open Internet Settings: {e}")))
}

pub fn capture() -> AppResult<ProxySnapshot> {
    let key = open(KEY_READ)?;
    Ok(ProxySnapshot {
        // Absent values are normal on a fresh profile: treat as "proxy off".
        enable: key.get_value("ProxyEnable").unwrap_or(0u32),
        server: key.get_value("ProxyServer").ok(),
        override_list: key.get_value("ProxyOverride").ok(),
    })
}

pub fn apply(addr: &str, bypass: &str) -> AppResult<()> {
    let key = open(KEY_WRITE)?;
    key.set_value("ProxyServer", &addr.to_string())
        .and_then(|_| key.set_value("ProxyOverride", &bypass.to_string()))
        .and_then(|_| key.set_value("ProxyEnable", &1u32))
        .map_err(|e| AppError::SystemProxy(format!("write proxy values: {e}")))?;
    notify();
    Ok(())
}

pub fn restore(snapshot: &ProxySnapshot) -> AppResult<()> {
    let key = open(KEY_WRITE)?;

    key.set_value("ProxyEnable", &snapshot.enable)
        .map_err(|e| AppError::SystemProxy(format!("restore ProxyEnable: {e}")))?;

    // A value we found absent is deleted again, not left behind as our own.
    match &snapshot.server {
        Some(value) => key
            .set_value("ProxyServer", value)
            .map_err(|e| AppError::SystemProxy(format!("restore ProxyServer: {e}")))?,
        None => {
            let _ = key.delete_value("ProxyServer");
        }
    }
    match &snapshot.override_list {
        Some(value) => key
            .set_value("ProxyOverride", value)
            .map_err(|e| AppError::SystemProxy(format!("restore ProxyOverride: {e}")))?,
        None => {
            let _ = key.delete_value("ProxyOverride");
        }
    }

    notify();
    Ok(())
}

/// True when the OS proxy is on and aimed at our own loopback address.
///
/// The check is what keeps recovery safe: a proxy the user configured
/// themselves points somewhere else, and must never be touched.
pub fn points_at_loopback() -> bool {
    let Ok(key) = open(KEY_READ) else {
        return false;
    };
    if key.get_value::<u32, _>("ProxyEnable").unwrap_or(0) != 1 {
        return false;
    }
    key.get_value::<String, _>("ProxyServer")
        .map(|server| server.contains("127.0.0.1"))
        .unwrap_or(false)
}

/// Switches the proxy off without claiming to know what preceded it.
pub fn disable() -> AppResult<()> {
    let key = open(KEY_WRITE)?;
    key.set_value("ProxyEnable", &0u32)
        .map_err(|e| AppError::SystemProxy(format!("disable proxy: {e}")))?;
    let _ = key.delete_value("ProxyServer");
    notify();
    Ok(())
}

/// Tells already-running WinINET consumers to re-read the settings. Without
/// this, open browsers keep using the old proxy until they restart.
fn notify() {
    // SAFETY: both calls take a null handle and a null buffer, which is the
    // documented form for these two notification-only options.
    unsafe {
        InternetSetOptionW(
            std::ptr::null_mut(),
            INTERNET_OPTION_SETTINGS_CHANGED,
            std::ptr::null_mut(),
            0,
        );
        InternetSetOptionW(
            std::ptr::null_mut(),
            INTERNET_OPTION_REFRESH,
            std::ptr::null_mut(),
            0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trips the real registry: capture, apply, restore, and confirm the
    /// user's original settings came back byte-for-byte.
    #[test]
    // Ignored by default: this mutates the machine-global WinINET settings,
    // which a running OpenWire session (or any other proxy app) also owns. A
    // routine `cargo test` must never switch a user's networking out from
    // under them. Run deliberately with `cargo test -- --ignored`.
    #[ignore]
    fn apply_then_restore_leaves_no_trace() {
        let _serial = super::super::lock_registry();
        let before = capture().expect("capture");
        apply("127.0.0.1:65000", super::super::LOOPBACK_BYPASS).expect("apply");

        let during = capture().expect("capture during");
        assert_eq!(during.enable, 1);
        assert_eq!(during.server.as_deref(), Some("127.0.0.1:65000"));

        assert!(points_at_loopback(), "our own proxy must be recognised");

        restore(&before).expect("restore");

        let after = capture().expect("capture after");
        assert_eq!(after.enable, before.enable);
        assert_eq!(after.server, before.server);
        assert_eq!(after.override_list, before.override_list);
    }

    /// Recovery must never reach for a proxy the user set up themselves.
    #[test]
    // Ignored by default: this mutates the machine-global WinINET settings,
    // which a running OpenWire session (or any other proxy app) also owns. A
    // routine `cargo test` must never switch a user's networking out from
    // under them. Run deliberately with `cargo test -- --ignored`.
    #[ignore]
    fn a_third_party_proxy_is_not_mistaken_for_ours() {
        let _serial = super::super::lock_registry();
        let before = capture().expect("capture");

        apply("proxy.corp.example:8080", super::super::LOOPBACK_BYPASS).expect("apply");
        assert!(
            !points_at_loopback(),
            "a non-loopback proxy must be left alone"
        );

        restore(&before).expect("restore");
    }
}
