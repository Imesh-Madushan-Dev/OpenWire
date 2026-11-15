//! Registering, removing and querying the tunnel service - the shared service
//! code in [`crate::core::winsvc`], applied to this one.

use crate::core::winsvc::{self, ServiceSpec};

pub use crate::core::winsvc::{exit, ServiceState};

use super::{FLAG_SERVICE, SERVICE_DISPLAY_NAME, SERVICE_NAME};

/// The driver DLL, copied beside the service's binary.
pub const WINTUN_DLL: &str = "wintun.dll";

pub const SPEC: ServiceSpec = ServiceSpec {
    name: SERVICE_NAME,
    display_name: SERVICE_DISPLAY_NAME,
    description: "Routes every app's traffic through OpenWire's tunnel while you are connected, \
                  with a virtual network adapter and leak protection. Idle otherwise.",
    dir_name: "OpenWire Tunnel",
    exe_name: "openwire-tunnel.exe",
    log_name: "tunnel.log",
    service_flag: FLAG_SERVICE,
    // Creating an adapter installs and loads a driver, and what that needs is
    // the driver installer's business; LocalSystem's set is kept.
    privileges: &[],
    // Routes and addresses go through these; starting before them fails.
    dependencies: &["Nsi", "TcpIp"],
    companions: &[WINTUN_DLL],
};

pub fn query() -> ServiceState {
    winsvc::query(&SPEC)
}

pub fn install_from_cli() -> i32 {
    match winsvc::install(&SPEC) {
        Ok(()) => exit::OK,
        Err(code) => code,
    }
}

pub fn uninstall_from_cli() -> i32 {
    // The driver goes too, unless something else - WireGuard, say - still has
    // an adapter on it. The DLL is loaded before its folder is removed, which
    // leaves that folder to be cleared again once it is let go.
    let wintun = SPEC
        .dir()
        .and_then(|dir| super::wintun::Wintun::load(&dir.join(WINTUN_DLL)).ok());
    let result = winsvc::uninstall(&SPEC);
    if let Some(wintun) = wintun {
        wintun.delete_driver();
        drop(wintun);
        if let Some(dir) = SPEC.dir() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
    match result {
        Ok(()) => exit::OK,
        Err(code) => code,
    }
}

/// Used once the app the service belongs to has been uninstalled.
pub fn remove_self() {
    winsvc::remove_self(&SPEC);
}
