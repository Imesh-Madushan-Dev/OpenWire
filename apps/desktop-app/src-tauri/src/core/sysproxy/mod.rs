//! System proxy control.
//!
//! Leak safety is the whole point of this module: whatever we change here is
//! captured in a [`ProxySnapshot`] first, and [`ProxyGuard`] restores it on
//! disconnect, on panic, and on process exit.
//!
//! In-memory guards cannot survive a process that is killed rather than asked
//! to stop - and `tauri dev`, Task Manager, and any crash all do exactly that.
//! So the snapshot is also written to disk before the OS is touched, and
//! [`recover`] undoes a stranded proxy on the next launch. Without that, a
//! single hard kill leaves the machine pointing at a dead port with no
//! networking at all.

use std::fs;
use std::path::Path;

use crate::core::store;
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

/// Serialises every test that touches the real OS proxy.
///
/// These tests share one machine-global registry key, so running them in
/// parallel makes them clobber each other's capture/restore pairs.
#[cfg(test)]
pub(crate) static REGISTRY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Takes the lock without propagating poisoning: one failing test must not
/// cascade into every other one reporting the same failure.
#[cfg(test)]
pub(crate) fn lock_registry() -> std::sync::MutexGuard<'static, ()> {
    REGISTRY_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// True when this platform can drive the OS proxy automatically.
pub fn is_supported() -> bool {
    imp::IS_SUPPORTED
}

/// What [`recover`] had to do, so the UI can say something specific.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    /// A stranded proxy was put back the way the user had it.
    Restored,
    /// The proxy was ours but the backup was gone; it was switched off instead.
    Cleared,
    /// Nothing was pointing at us.
    NothingToDo,
}

/// Undoes a proxy left behind by a previous run.
///
/// Only touches settings that still point at our own loopback address - if the
/// user has since configured a real proxy, that is theirs and must survive.
pub fn recover(backup: &Path) -> AppResult<Recovery> {
    let stored: Option<ProxySnapshot> = store::read_json(backup)?;
    let ours = imp::points_at_loopback();

    let outcome = match (stored, ours) {
        (Some(snapshot), true) => {
            imp::restore(&snapshot)?;
            log::info!("restored a system proxy stranded by a previous run");
            Recovery::Restored
        }
        (None, true) => {
            // The backup is gone but the machine is still pointed at a port
            // nothing is listening on. Switching it off beats leaving the user
            // with no networking.
            imp::disable()?;
            log::warn!("cleared a stranded system proxy with no backup on disk");
            Recovery::Cleared
        }
        _ => Recovery::NothingToDo,
    };

    let _ = fs::remove_file(backup);
    Ok(outcome)
}

/// RAII handle. Dropping it puts the user's network settings back.
#[derive(Debug)]
pub struct ProxyGuard {
    snapshot: Option<ProxySnapshot>,
    backup: std::path::PathBuf,
}

impl ProxyGuard {
    /// Points the OS at `127.0.0.1:port`, remembering what was there before.
    pub fn engage(port: u16, lan_bypass: bool, backup: &Path) -> AppResult<Self> {
        let snapshot = imp::capture()?;

        // Persisted *before* the OS is touched: a crash in the microseconds
        // that follow must still leave a recoverable trail.
        if let Err(err) = store::write_json(backup, &snapshot) {
            log::warn!("could not save a proxy backup ({err}); continuing without one");
        }

        if let Err(err) = imp::apply(&format!("127.0.0.1:{port}"), bypass_list(lan_bypass)) {
            // A partial write would otherwise strand the very settings we were
            // trying to protect.
            let _ = imp::restore(&snapshot);
            let _ = fs::remove_file(backup);
            return Err(err);
        }

        log::info!("system proxy pointed at 127.0.0.1:{port}");
        Ok(Self {
            snapshot: Some(snapshot),
            backup: backup.to_path_buf(),
        })
    }

    /// Restores eagerly and reports failure, instead of swallowing it in `Drop`.
    pub fn release(mut self) -> AppResult<()> {
        match self.snapshot.take() {
            Some(snapshot) => {
                let result = imp::restore(&snapshot);
                // The backup only exists to survive an unclean exit; this was a
                // clean one either way.
                let _ = fs::remove_file(&self.backup);
                result
            }
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
                let _ = fs::remove_file(&self.backup);
            }
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// The regression this whole module's persistence exists for: a process
    /// that is killed rather than asked to stop leaves the machine pointing at
    /// a dead port, and every site stops loading.
    #[test]
    // Ignored by default: this mutates the machine-global WinINET settings,
    // which a running OpenWire session (or any other proxy app) also owns. A
    // routine `cargo test` must never switch a user's networking out from
    // under them. Run deliberately with `cargo test -- --ignored`.
    #[ignore]
    fn a_proxy_stranded_by_a_hard_kill_is_recovered() {
        let _serial = lock_registry();
        let backup = std::env::temp_dir().join("openwire-test-proxy-backup.json");
        let _ = fs::remove_file(&backup);

        let guard = ProxyGuard::engage(65001, false, &backup).expect("engage");
        assert!(
            backup.exists(),
            "engage must leave a recoverable trail on disk"
        );
        assert!(imp::points_at_loopback(), "the OS should now point at us");

        // Exactly what a taskkill does: the guard never gets to run its Drop.
        std::mem::forget(guard);

        assert_eq!(recover(&backup).expect("recover"), Recovery::Restored);
        assert!(
            !imp::points_at_loopback(),
            "recovery must hand networking back"
        );
        assert!(!backup.exists(), "a spent backup must not linger");

        // Idempotent: a second launch has nothing left to undo.
        assert_eq!(recover(&backup).expect("recover"), Recovery::NothingToDo);
    }

    /// Without a backup we cannot know what preceded us, but leaving the user
    /// with no networking is never the better option.
    #[test]
    // Ignored by default: this mutates the machine-global WinINET settings,
    // which a running OpenWire session (or any other proxy app) also owns. A
    // routine `cargo test` must never switch a user's networking out from
    // under them. Run deliberately with `cargo test -- --ignored`.
    #[ignore]
    fn a_stranded_proxy_with_no_backup_is_switched_off() {
        let _serial = lock_registry();
        let backup = std::env::temp_dir().join("openwire-test-proxy-missing.json");
        let _ = fs::remove_file(&backup);

        let before = imp::capture().expect("capture");
        imp::apply("127.0.0.1:65002", LOOPBACK_BYPASS).expect("apply");

        assert_eq!(recover(&backup).expect("recover"), Recovery::Cleared);
        assert!(!imp::points_at_loopback());

        imp::restore(&before).expect("restore");
    }
}
