//! System proxy control.
//!
//! Leak safety is the whole point of this module: whatever we change here is
//! captured in a [`ProxySnapshot`] first, and [`ProxyGuard`] restores it on
//! disconnect, on panic, and on process exit. If the app dies mid-session the
//! user must not be left with a dead proxy wired into their OS.

use crate::error::AppResult;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

#[cfg(not(windows))]
mod unsupported;
#[cfg(not(windows))]
use unsupported as imp;

/// Destinations that must never be tunnelled. Loopback is mandatory (the proxy
/// would otherwise route to itself); private ranges are the LAN bypass.
pub const LOOPBACK_BYPASS: &str = "localhost;127.*;<local>";
pub const LAN_BYPASS: &str = "localhost;127.*;10.*;172.16.*;172.17.*;172.18.*;172.19.*;172.20.*;172.21.*;172.22.*;172.23.*;172.24.*;172.25.*;172.26.*;172.27.*;172.28.*;172.29.*;172.30.*;172.31.*;192.168.*;169.254.*;<local>";

pub fn bypass_list(lan_bypass: bool) -> &'static str {
    if lan_bypass {
        LAN_BYPASS
    } else {
        LOOPBACK_BYPASS
    }
}

/// Previous OS proxy configuration, captured before we touch anything.
pub type ProxySnapshot = imp::ProxySnapshot;

/// True when this platform can drive the OS proxy automatically.
pub fn is_supported() -> bool {
    imp::IS_SUPPORTED
}

/// RAII handle. Dropping it puts the user's network settings back.
#[derive(Debug)]
pub struct ProxyGuard {
    snapshot: Option<ProxySnapshot>,
}

impl ProxyGuard {
    /// Points the OS at `127.0.0.1:port`, remembering what was there before.
    pub fn engage(port: u16, lan_bypass: bool) -> AppResult<Self> {
        let snapshot = imp::capture()?;
        imp::apply(&format!("127.0.0.1:{port}"), bypass_list(lan_bypass))?;
        log::info!("system proxy pointed at 127.0.0.1:{port}");
        Ok(Self {
            snapshot: Some(snapshot),
        })
    }

    /// Restores eagerly and reports failure, instead of swallowing it in `Drop`.
    pub fn release(mut self) -> AppResult<()> {
        match self.snapshot.take() {
            Some(snapshot) => imp::restore(&snapshot),
            None => Ok(()),
        }
    }
}

impl Drop for ProxyGuard {
    fn drop(&mut self) {
        if let Some(snapshot) = self.snapshot.take() {
            // Last line of defence on an abnormal exit: log, never panic.
            if let Err(err) = imp::restore(&snapshot) {
                log::error!("failed to restore system proxy on drop: {err}");
            } else {
                log::info!("system proxy restored");
            }
        }
    }
}
