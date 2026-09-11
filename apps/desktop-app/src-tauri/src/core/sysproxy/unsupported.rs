//! Non-Windows fallback.
//!
//! ponytail: macOS (`networksetup -setwebproxy`) and GNOME (`gsettings set
//! org.gnome.system.proxy`) both need per-network-service enumeration and
//! elevated prompts. Until this ships beyond Windows, the app reports the
//! loopback address and lets the user point their browser at it manually -
//! which works today and leaks nothing.

use crate::error::AppResult;

pub const IS_SUPPORTED: bool = false;

#[derive(Debug, Clone)]
pub struct ProxySnapshot;

pub fn capture() -> AppResult<ProxySnapshot> {
    Ok(ProxySnapshot)
}

pub fn apply(addr: &str, _bypass: &str) -> AppResult<()> {
    log::info!("automatic system proxy unsupported here; configure apps to use {addr}");
    Ok(())
}

pub fn restore(_snapshot: &ProxySnapshot) -> AppResult<()> {
    Ok(())
}
