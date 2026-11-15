//! Running as a service: the SCM handshake every OpenWire service shares.
//!
//! The SCM calls back into plain functions, not closures, so what differs
//! between services is registered once in a static before the dispatcher
//! starts. One process hosts one service, so that is all the state needed.

use std::ffi::c_void;
use std::sync::OnceLock;

use tokio::sync::watch;
use windows_sys::Win32::Foundation::{ERROR_CALL_NOT_IMPLEMENTED, NO_ERROR};
use windows_sys::Win32::System::Services::{
    RegisterServiceCtrlHandlerExW, SetServiceStatus, StartServiceCtrlDispatcherW,
    SERVICE_ACCEPT_SHUTDOWN, SERVICE_ACCEPT_STOP, SERVICE_CONTROL_INTERROGATE,
    SERVICE_CONTROL_SHUTDOWN, SERVICE_CONTROL_STOP, SERVICE_RUNNING, SERVICE_START_PENDING,
    SERVICE_STATUS, SERVICE_STATUS_HANDLE, SERVICE_STOPPED, SERVICE_STOP_PENDING,
    SERVICE_TABLE_ENTRYW, SERVICE_WIN32_OWN_PROCESS,
};

use super::wide;

pub struct Service {
    pub name: &'static str,
    /// The service's body. Returns once `stop` turns true and it has wound
    /// down; the service is reported stopped after that.
    pub main: fn(stop: watch::Receiver<bool>),
    /// Called on a stop request, after `stop` is set, for anything a watch
    /// channel cannot wake - a thread blocked in a system call, say.
    pub on_stop: fn(),
}

static SERVICE: OnceLock<Service> = OnceLock::new();
static STATUS_HANDLE: OnceLock<usize> = OnceLock::new();
static STOP: OnceLock<watch::Sender<bool>> = OnceLock::new();

/// Hands this thread to the SCM, which runs the service on another and
/// returns here once it has stopped.
pub fn run(service: Service) -> i32 {
    let name = service.name;
    if SERVICE.set(service).is_err() {
        return 1;
    }

    let mut name = wide(name);
    let table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: name.as_mut_ptr(),
            lpServiceProc: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW {
            lpServiceName: std::ptr::null_mut(),
            lpServiceProc: None,
        },
    ];

    // SAFETY: the table is NUL-entry terminated and outlives the call, which
    // blocks until every service in it has stopped.
    if unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } == 0 {
        // Run by hand rather than by the SCM.
        log::error!(
            "not started by the service manager: {}",
            std::io::Error::last_os_error()
        );
        return 1;
    }
    0
}

/// Asks the service to wind down, as a stop from the SCM would.
pub fn request_stop() {
    if let Some(stop) = STOP.get() {
        let _ = stop.send(true);
    }
    if let Some(service) = SERVICE.get() {
        (service.on_stop)();
    }
}

unsafe extern "system" fn service_main(_argc: u32, _argv: *mut windows_sys::core::PWSTR) {
    let Some(service) = SERVICE.get() else {
        return;
    };
    let name = wide(service.name);
    let handle = RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(on_control), std::ptr::null());
    if handle.is_null() {
        log::error!("could not register the control handler");
        return;
    }
    let _ = STATUS_HANDLE.set(handle as usize);
    report(SERVICE_START_PENDING, 0);

    let (stop_tx, stop_rx) = watch::channel(false);
    let _ = STOP.set(stop_tx);

    report(SERVICE_RUNNING, 0);
    log::info!("started");
    (service.main)(stop_rx);
    log::info!("stopped");
    report(SERVICE_STOPPED, 0);
}

unsafe extern "system" fn on_control(
    control: u32,
    _event: u32,
    _data: *mut c_void,
    _context: *mut c_void,
) -> u32 {
    match control {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
            report(SERVICE_STOP_PENDING, 0);
            request_stop();
            NO_ERROR
        }
        SERVICE_CONTROL_INTERROGATE => NO_ERROR,
        _ => ERROR_CALL_NOT_IMPLEMENTED,
    }
}

fn report(state: u32, exit_code: u32) {
    let Some(handle) = STATUS_HANDLE.get() else {
        return;
    };
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: if state == SERVICE_RUNNING {
            SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN
        } else {
            0
        },
        dwWin32ExitCode: exit_code,
        dwServiceSpecificExitCode: 0,
        dwCheckPoint: 0,
        dwWaitHint: if state == SERVICE_RUNNING || state == SERVICE_STOPPED {
            0
        } else {
            5_000
        },
    };
    // SAFETY: the handle came from RegisterServiceCtrlHandlerExW and is never
    // closed; SetServiceStatus may be called from any thread.
    unsafe { SetServiceStatus(*handle as SERVICE_STATUS_HANDLE, &status) };
}
