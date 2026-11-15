//! Registering, removing and querying the monitor service - the shared
//! service code in [`crate::core::winsvc`], applied to this one.
//!
//! `install` and `uninstall` run in the elevated helper (`--netmon-install`,
//! `--netmon-uninstall`); `query` and `elevate` run in the app, unelevated.

use crate::core::winsvc::{self, ServiceSpec};

pub use crate::core::winsvc::{elevate, exit, ElevationError, ServiceState};

use super::{FLAG_SERVICE, SERVICE_DISPLAY_NAME, SERVICE_NAME};

pub const SPEC: ServiceSpec = ServiceSpec {
    name: SERVICE_NAME,
    display_name: SERVICE_DISPLAY_NAME,
    description: "Counts network traffic by app, host and traffic type for OpenWire's Usage page. \
                  Reads Windows' own network events; sends nothing off this device.",
    dir_name: "OpenWire Network Monitor",
    exe_name: "openwire-netmon.exe",
    log_name: "netmon.log",
    service_flag: FLAG_SERVICE,
    // LocalSystem is needed to trace, but not its two dozen privileges.
    privileges: &["SeChangeNotifyPrivilege", "SeSystemProfilePrivilege"],
    dependencies: &[],
    companions: &[],
};

/// Needs no rights beyond what every user has on a service by default.
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
    match winsvc::uninstall(&SPEC) {
        Ok(()) => exit::OK,
        Err(code) => code,
    }
}

/// Used once the app the service belongs to has been uninstalled.
pub fn remove_self() {
    winsvc::remove_self(&SPEC);
}
