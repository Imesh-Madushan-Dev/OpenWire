//! Registering, removing and querying the service.
//!
//! `install` and `uninstall` run in the elevated helper (`--netmon-install`,
//! `--netmon-uninstall`); `query` and `elevate` run in the app, unelevated.

use std::path::Path;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_CANCELLED, ERROR_SERVICE_ALREADY_RUNNING,
    ERROR_SERVICE_DOES_NOT_EXIST, WAIT_OBJECT_0,
};
use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, DELETE, MOVEFILE_DELAY_UNTIL_REBOOT};
use windows_sys::Win32::System::Services::{
    ChangeServiceConfig2W, ChangeServiceConfigW, CloseServiceHandle, ControlService,
    CreateServiceW, DeleteService, OpenSCManagerW, OpenServiceW, QueryServiceStatusEx,
    StartServiceW, SC_ACTION, SC_ACTION_NONE, SC_ACTION_RESTART, SC_HANDLE, SC_MANAGER_CONNECT,
    SC_MANAGER_CREATE_SERVICE, SC_STATUS_PROCESS_INFO, SERVICE_ALL_ACCESS, SERVICE_AUTO_START,
    SERVICE_CONFIG_DESCRIPTION, SERVICE_CONFIG_FAILURE_ACTIONS,
    SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO, SERVICE_CONFIG_SERVICE_SID_INFO, SERVICE_CONTROL_STOP,
    SERVICE_DESCRIPTIONW, SERVICE_ERROR_NORMAL, SERVICE_FAILURE_ACTIONSW, SERVICE_NO_CHANGE,
    SERVICE_QUERY_STATUS, SERVICE_REQUIRED_PRIVILEGES_INFOW, SERVICE_RUNNING, SERVICE_SID_INFO,
    SERVICE_SID_TYPE_UNRESTRICTED, SERVICE_STATUS, SERVICE_STATUS_PROCESS, SERVICE_STOP,
    SERVICE_STOPPED, SERVICE_WIN32_OWN_PROCESS,
};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};

use super::sys::{self, wide};
use super::{FLAG_OWNER, FLAG_SERVICE, SERVICE_DISPLAY_NAME, SERVICE_NAME};

/// Exit codes of the elevated helper, mapped back to words by the app.
pub mod exit {
    pub const OK: i32 = 0;
    pub const NO_PROGRAM_FILES: i32 = 10;
    pub const COPY_FAILED: i32 = 11;
    pub const REGISTER_FAILED: i32 = 12;
    pub const START_FAILED: i32 = 13;
    pub const REMOVE_FAILED: i32 = 14;
}

/// Closes an SCM handle on drop.
struct Handle(SC_HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: only ever constructed around a non-null handle we own.
        unsafe { CloseServiceHandle(self.0) };
    }
}

fn open_manager(access: u32) -> Option<Handle> {
    // SAFETY: null machine and database mean the local, active database.
    let handle = unsafe { OpenSCManagerW(std::ptr::null(), std::ptr::null(), access) };
    (!handle.is_null()).then_some(Handle(handle))
}

fn open_service(manager: &Handle, access: u32) -> Result<Handle, u32> {
    let name = wide(SERVICE_NAME);
    // SAFETY: a valid manager handle and a NUL-terminated name.
    let handle = unsafe { OpenServiceW(manager.0, name.as_ptr(), access) };
    if handle.is_null() {
        Err(unsafe { GetLastError() })
    } else {
        Ok(Handle(handle))
    }
}

fn status_of(service: &Handle) -> Option<SERVICE_STATUS_PROCESS> {
    let mut status = SERVICE_STATUS_PROCESS::default();
    let mut needed = 0u32;
    // SAFETY: the buffer is exactly one SERVICE_STATUS_PROCESS.
    let ok = unsafe {
        QueryServiceStatusEx(
            service.0,
            SC_STATUS_PROCESS_INFO,
            &mut status as *mut SERVICE_STATUS_PROCESS as *mut u8,
            std::mem::size_of::<SERVICE_STATUS_PROCESS>() as u32,
            &mut needed,
        )
    };
    (ok != 0).then_some(status)
}

// ---------------------------------------------------------------------------
// Unelevated: status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServiceState {
    pub installed: bool,
    pub running: bool,
    /// The service's process, for checking the pipe really is its.
    pub pid: u32,
}

/// Needs no rights beyond what every user has on a service by default.
pub fn query() -> ServiceState {
    let Some(manager) = open_manager(SC_MANAGER_CONNECT) else {
        return ServiceState::default();
    };
    let Ok(service) = open_service(&manager, SERVICE_QUERY_STATUS) else {
        return ServiceState::default();
    };
    let status = status_of(&service).unwrap_or_default();
    ServiceState {
        installed: true,
        running: status.dwCurrentState == SERVICE_RUNNING,
        pid: status.dwProcessId,
    }
}

// ---------------------------------------------------------------------------
// Unelevated: asking for elevation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevationError {
    /// The user dismissed the UAC prompt.
    Declined,
    /// The helper could not be launched at all.
    Launch,
    /// The helper ran and failed, with its exit code.
    Failed(i32),
    TimedOut,
}

/// Runs this executable elevated with `flag`, waiting for it to finish.
/// Blocking: Windows shows the UAC prompt and the user takes their time.
pub fn elevate(flag: &str) -> Result<(), ElevationError> {
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS,
        SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let exe = std::env::current_exe().map_err(|_| ElevationError::Launch)?;
    let file = wide(&exe);
    let verb = wide("runas");
    let parameters = wide(flag);

    // SAFETY: zero is a valid starting state for this plain-data struct.
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = parameters.as_ptr();
    info.nShow = SW_HIDE;

    // SAFETY: every string outlives the call.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(match unsafe { GetLastError() } {
            ERROR_CANCELLED => ElevationError::Declined,
            _ => ElevationError::Launch,
        });
    }
    if info.hProcess.is_null() {
        return Err(ElevationError::Launch);
    }

    // Two minutes covers stopping an old service, copying and starting a new
    // one many times over; past that something has hung.
    let waited = unsafe { WaitForSingleObject(info.hProcess, 120_000) };
    let mut code = 1u32;
    let finished =
        waited == WAIT_OBJECT_0 && unsafe { GetExitCodeProcess(info.hProcess, &mut code) } != 0;
    unsafe { CloseHandle(info.hProcess) };

    match (finished, code as i32) {
        (false, _) => Err(ElevationError::TimedOut),
        (true, exit::OK) => Ok(()),
        (true, code) => Err(ElevationError::Failed(code)),
    }
}

// ---------------------------------------------------------------------------
// Elevated: install and uninstall
// ---------------------------------------------------------------------------

pub fn install_from_cli() -> i32 {
    match install() {
        Ok(()) => exit::OK,
        Err(code) => code,
    }
}

pub fn uninstall_from_cli() -> i32 {
    match uninstall() {
        Ok(()) => exit::OK,
        Err(code) => code,
    }
}

/// Copies this binary somewhere only administrators can write, and registers
/// that copy as an auto-start service. Re-running it upgrades in place.
fn install() -> Result<(), i32> {
    let source = std::env::current_exe().map_err(|_| exit::COPY_FAILED)?;
    let dir = sys::service_dir().ok_or(exit::NO_PROGRAM_FILES)?;
    let target = sys::service_exe().ok_or(exit::NO_PROGRAM_FILES)?;

    let manager = open_manager(SC_MANAGER_CONNECT | SC_MANAGER_CREATE_SERVICE)
        .ok_or(exit::REGISTER_FAILED)?;
    let existing = open_service(&manager, SERVICE_ALL_ACCESS).ok();

    // The running copy holds its file open; it has to stop before it can be
    // replaced.
    if let Some(service) = &existing {
        stop_and_wait(service);
    }
    copy_binary(&source, &dir, &target)?;

    // Quoted: an unquoted path with spaces is the textbook service hijack.
    let command = format!(
        "\"{}\" {FLAG_SERVICE} {FLAG_OWNER} \"{}\"",
        target.display(),
        source.display()
    );
    let command = wide(&command);
    let display = wide(SERVICE_DISPLAY_NAME);

    let service = match existing {
        Some(service) => {
            // SAFETY: valid handle; null leaves a field unchanged.
            let ok = unsafe {
                ChangeServiceConfigW(
                    service.0,
                    SERVICE_NO_CHANGE,
                    SERVICE_AUTO_START,
                    SERVICE_NO_CHANGE,
                    command.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    display.as_ptr(),
                )
            };
            if ok == 0 {
                return Err(exit::REGISTER_FAILED);
            }
            service
        }
        None => {
            let name = wide(SERVICE_NAME);
            // SAFETY: every string outlives the call; a null account is
            // LocalSystem, trimmed to the privileges below.
            let handle = unsafe {
                CreateServiceW(
                    manager.0,
                    name.as_ptr(),
                    display.as_ptr(),
                    SERVICE_ALL_ACCESS,
                    SERVICE_WIN32_OWN_PROCESS,
                    SERVICE_AUTO_START,
                    SERVICE_ERROR_NORMAL,
                    command.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                )
            };
            if handle.is_null() {
                return Err(exit::REGISTER_FAILED);
            }
            Handle(handle)
        }
    };

    configure(&service);

    // SAFETY: valid handle, no arguments.
    if unsafe { StartServiceW(service.0, 0, std::ptr::null()) } == 0
        && unsafe { GetLastError() } != ERROR_SERVICE_ALREADY_RUNNING
    {
        return Err(exit::START_FAILED);
    }
    Ok(())
}

/// Description, restart-on-crash, and least privilege. Best effort: a service
/// missing its description still works.
fn configure(service: &Handle) {
    let mut description = wide(
        "Counts network traffic by app, host and traffic type for Maizy VPN's Usage page. \
         Reads Windows' own network events; sends nothing off this device.",
    );
    let description = SERVICE_DESCRIPTIONW {
        lpDescription: description.as_mut_ptr(),
    };

    // Restart after 5 s, then 30 s, then leave it; the count resets daily.
    let mut actions = [
        SC_ACTION {
            Type: SC_ACTION_RESTART,
            Delay: 5_000,
        },
        SC_ACTION {
            Type: SC_ACTION_RESTART,
            Delay: 30_000,
        },
        SC_ACTION {
            Type: SC_ACTION_NONE,
            Delay: 0,
        },
    ];
    let failure = SERVICE_FAILURE_ACTIONSW {
        dwResetPeriod: 86_400,
        lpRebootMsg: std::ptr::null_mut(),
        lpCommand: std::ptr::null_mut(),
        cActions: actions.len() as u32,
        lpsaActions: actions.as_mut_ptr(),
    };

    // LocalSystem is needed to trace, but not its two dozen privileges. The
    // SCM strips everything not listed here from the service's token.
    let mut privileges: Vec<u16> = ["SeChangeNotifyPrivilege", "SeSystemProfilePrivilege"]
        .iter()
        .flat_map(wide)
        .chain(std::iter::once(0))
        .collect();
    let privileges = SERVICE_REQUIRED_PRIVILEGES_INFOW {
        pmszRequiredPrivileges: privileges.as_mut_ptr(),
    };
    let sid = SERVICE_SID_INFO {
        dwServiceSidType: SERVICE_SID_TYPE_UNRESTRICTED,
    };

    // SAFETY: each info struct matches its level and outlives its call.
    unsafe {
        ChangeServiceConfig2W(
            service.0,
            SERVICE_CONFIG_DESCRIPTION,
            &description as *const _ as _,
        );
        ChangeServiceConfig2W(
            service.0,
            SERVICE_CONFIG_FAILURE_ACTIONS,
            &failure as *const _ as _,
        );
        ChangeServiceConfig2W(
            service.0,
            SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO,
            &privileges as *const _ as _,
        );
        ChangeServiceConfig2W(
            service.0,
            SERVICE_CONFIG_SERVICE_SID_INFO,
            &sid as *const _ as _,
        );
    }
}

fn copy_binary(source: &Path, dir: &Path, target: &Path) -> Result<(), i32> {
    std::fs::create_dir_all(dir).map_err(|_| exit::COPY_FAILED)?;
    let staging = target.with_extension("exe.new");
    std::fs::copy(source, &staging).map_err(|_| exit::COPY_FAILED)?;

    // A just-stopped process can hold its image open for a moment longer.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match std::fs::rename(&staging, target) {
            Ok(()) => return Ok(()),
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(250)),
            Err(_) => {
                let _ = std::fs::remove_file(&staging);
                return Err(exit::COPY_FAILED);
            }
        }
    }
}

fn uninstall() -> Result<(), i32> {
    let manager = open_manager(SC_MANAGER_CONNECT).ok_or(exit::REMOVE_FAILED)?;
    match open_service(&manager, SERVICE_STOP | SERVICE_QUERY_STATUS | DELETE) {
        Ok(service) => {
            stop_and_wait(&service);
            // SAFETY: valid handle opened with DELETE.
            if unsafe { DeleteService(service.0) } == 0 {
                return Err(exit::REMOVE_FAILED);
            }
        }
        // Already gone counts as done.
        Err(ERROR_SERVICE_DOES_NOT_EXIST) => {}
        Err(_) => return Err(exit::REMOVE_FAILED),
    }
    remove_files();
    Ok(())
}

fn remove_files() {
    let Some(dir) = sys::service_dir() else {
        return;
    };
    if std::fs::remove_dir_all(&dir).is_ok() {
        return;
    }
    // Still locked: leave it to the next reboot rather than fail the uninstall.
    for path in [dir.join("maizy-netmon.exe"), dir.join("netmon.log"), dir] {
        let path = wide(&path);
        // SAFETY: a NUL-terminated path; a null target means "delete".
        unsafe { MoveFileExW(path.as_ptr(), std::ptr::null(), MOVEFILE_DELAY_UNTIL_REBOOT) };
    }
}

fn stop_and_wait(service: &Handle) {
    let mut status = SERVICE_STATUS::default();
    // SAFETY: valid handle; failure (already stopped) is fine.
    unsafe { ControlService(service.0, SERVICE_CONTROL_STOP, &mut status) };

    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        match status_of(service) {
            Some(state) if state.dwCurrentState != SERVICE_STOPPED => {
                std::thread::sleep(Duration::from_millis(200));
            }
            _ => return,
        }
    }
}

// ---------------------------------------------------------------------------
// From inside the service
// ---------------------------------------------------------------------------

/// Used once the app the service belongs to has been uninstalled: marks the
/// service for deletion (completed by the SCM when it stops) and its files
/// for removal at the next reboot, since a running image cannot delete itself.
pub fn remove_self() {
    if let Some(manager) = open_manager(SC_MANAGER_CONNECT) {
        if let Ok(service) = open_service(&manager, DELETE) {
            // SAFETY: valid handle opened with DELETE.
            unsafe { DeleteService(service.0) };
        }
    }
    remove_files();
}
