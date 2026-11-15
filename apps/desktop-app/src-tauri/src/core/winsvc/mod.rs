//! Windows services that OpenWire installs: registering, removing, querying
//! and elevating, shared by every service the app ships.
//!
//! Each service is described once by a [`ServiceSpec`]. They all follow the
//! same rules, which is why they share this code rather than each having a
//! copy:
//!
//! - the service runs from its own copy of the app binary under Program
//!   Files, a folder only administrators can write to. The app may be
//!   installed somewhere the user can write, and a SYSTEM service started
//!   from there would let anything running as the user replace it and
//!   inherit SYSTEM;
//! - the copy is told which app executable installed it (`--owner`), so it
//!   can remove itself once that app has been uninstalled, and so a service
//!   that takes commands can check who is asking;
//! - install and uninstall run in a short-lived elevated helper - the same
//!   binary, launched through UAC with a flag - and report back through an
//!   exit code, mapped to words by the app.
//!
//! `install`, `uninstall` and `remove_self` run elevated; `query` and
//! `elevate` run in the app, unelevated.

pub mod host;
pub mod log;
pub mod pipe;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
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

/// Passed to every service: the app executable it was installed from.
pub const FLAG_OWNER: &str = "--owner";

/// Everything that differs between OpenWire's services.
#[derive(Debug)]
pub struct ServiceSpec {
    pub name: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    /// Folder under Program Files holding the service's copy of the binary.
    pub dir_name: &'static str,
    /// File name of that copy, so Task Manager shows what it is.
    pub exe_name: &'static str,
    /// The service's own log, beside its binary.
    pub log_name: &'static str,
    /// The flag the copy is started with, which selects its role.
    pub service_flag: &'static str,
    /// The privileges the SCM leaves in the service's token. Empty keeps
    /// LocalSystem's own set, for services whose needs depend on a driver.
    pub privileges: &'static [&'static str],
    /// Services that must be running first.
    pub dependencies: &'static [&'static str],
    /// Files from beside the app's executable that the service needs beside
    /// its own copy - a DLL it loads, say.
    pub companions: &'static [&'static str],
}

impl ServiceSpec {
    pub fn dir(&self) -> Option<PathBuf> {
        program_files().map(|root| root.join(self.dir_name))
    }

    pub fn exe(&self) -> Option<PathBuf> {
        self.dir().map(|dir| dir.join(self.exe_name))
    }

    pub fn log_path(&self) -> Option<PathBuf> {
        self.dir().map(|dir| dir.join(self.log_name))
    }
}

/// Exit codes of the elevated helper, mapped back to words by the app.
pub mod exit {
    pub const OK: i32 = 0;
    pub const NO_PROGRAM_FILES: i32 = 10;
    pub const COPY_FAILED: i32 = 11;
    pub const REGISTER_FAILED: i32 = 12;
    pub const START_FAILED: i32 = 13;
    pub const REMOVE_FAILED: i32 = 14;
    /// A file the service needs was not found beside the app.
    pub const COMPANION_MISSING: i32 = 15;
}

/// NUL-terminated UTF-16, for every `PCWSTR` parameter.
pub fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// `C:\Program Files`, read from the known-folder registry rather than the
/// environment: the elevated installer must not be steerable into a folder
/// the user can write to by a doctored `%ProgramFiles%`.
pub fn program_files() -> Option<PathBuf> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramFiles, SHGetKnownFolderPath};

    let mut raw: windows_sys::core::PWSTR = std::ptr::null_mut();
    // SAFETY: the out-pointer is freed with CoTaskMemFree whatever the result,
    // as the API requires.
    let hr =
        unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramFiles, 0, std::ptr::null_mut(), &mut raw) };
    if raw.is_null() {
        return None;
    }

    let path = if hr >= 0 {
        // SAFETY: on success `raw` is a NUL-terminated string.
        let len = (0..).take_while(|&i| unsafe { *raw.add(i) } != 0).count();
        Some(PathBuf::from(String::from_utf16_lossy(unsafe {
            std::slice::from_raw_parts(raw, len)
        })))
    } else {
        None
    };
    unsafe { CoTaskMemFree(raw.cast()) };
    path
}

/// The `--owner` path a service was started with.
pub fn owner_from_args() -> Option<PathBuf> {
    let mut args = std::env::args().skip_while(|arg| arg != FLAG_OWNER).skip(1);
    args.next().map(PathBuf::from)
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

fn open_service(manager: &Handle, name: &str, access: u32) -> Result<Handle, u32> {
    let name = wide(name);
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
    /// The service's process, for checking a pipe really is its.
    pub pid: u32,
}

/// Needs no rights beyond what every user has on a service by default.
pub fn query(spec: &ServiceSpec) -> ServiceState {
    let Some(manager) = open_manager(SC_MANAGER_CONNECT) else {
        return ServiceState::default();
    };
    let Ok(service) = open_service(&manager, spec.name, SERVICE_QUERY_STATUS) else {
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

/// Copies this binary (and its companions) somewhere only administrators can
/// write, and registers that copy as an auto-start service. Re-running it
/// upgrades in place.
pub fn install(spec: &ServiceSpec) -> Result<(), i32> {
    let source = std::env::current_exe().map_err(|_| exit::COPY_FAILED)?;
    let dir = spec.dir().ok_or(exit::NO_PROGRAM_FILES)?;
    let target = spec.exe().ok_or(exit::NO_PROGRAM_FILES)?;

    // Found before anything is stopped, so a broken app folder leaves the
    // running service alone rather than half-replaced.
    let companions = spec
        .companions
        .iter()
        .map(|name| companion_source(&source, name).map(|path| (path, *name)))
        .collect::<Option<Vec<_>>>()
        .ok_or(exit::COMPANION_MISSING)?;

    let manager = open_manager(SC_MANAGER_CONNECT | SC_MANAGER_CREATE_SERVICE)
        .ok_or(exit::REGISTER_FAILED)?;
    let existing = open_service(&manager, spec.name, SERVICE_ALL_ACCESS).ok();

    // The running copy holds its files open; it has to stop before they can
    // be replaced.
    if let Some(service) = &existing {
        stop_and_wait(service);
    }
    std::fs::create_dir_all(&dir).map_err(|_| exit::COPY_FAILED)?;
    copy_replacing(&source, &target)?;
    for (path, name) in &companions {
        copy_replacing(path, &dir.join(name))?;
    }

    // Quoted: an unquoted path with spaces is the textbook service hijack.
    let command = format!(
        "\"{}\" {} {FLAG_OWNER} \"{}\"",
        target.display(),
        spec.service_flag,
        source.display()
    );
    let command = wide(&command);
    let display = wide(spec.display_name);
    // A double-NUL-terminated list, or null for none.
    let dependencies: Option<Vec<u16>> = (!spec.dependencies.is_empty()).then(|| {
        spec.dependencies
            .iter()
            .flat_map(wide)
            .chain(std::iter::once(0))
            .collect()
    });
    let dependencies_ptr = dependencies
        .as_ref()
        .map_or(std::ptr::null(), |list| list.as_ptr());

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
                    dependencies_ptr,
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
            let name = wide(spec.name);
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
                    dependencies_ptr,
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

    configure(spec, &service);

    // SAFETY: valid handle, no arguments.
    if unsafe { StartServiceW(service.0, 0, std::ptr::null()) } == 0
        && unsafe { GetLastError() } != ERROR_SERVICE_ALREADY_RUNNING
    {
        return Err(exit::START_FAILED);
    }
    Ok(())
}

/// Where a companion file sits beside the app: next to the executable in an
/// installed build, or under `resources/` in a development tree.
fn companion_source(exe: &Path, name: &str) -> Option<PathBuf> {
    let dir = exe.parent()?;
    [dir.join(name), dir.join("resources").join(name)]
        .into_iter()
        .find(|candidate| candidate.is_file())
}

/// Description, restart-on-crash, and least privilege. Best effort: a service
/// missing its description still works.
fn configure(spec: &ServiceSpec, service: &Handle) {
    let mut description = wide(spec.description);
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
            SERVICE_CONFIG_SERVICE_SID_INFO,
            &sid as *const _ as _,
        );
    }

    // LocalSystem carries two dozen privileges; the SCM strips everything not
    // listed here from the service's token.
    if !spec.privileges.is_empty() {
        let mut privileges: Vec<u16> = spec
            .privileges
            .iter()
            .flat_map(wide)
            .chain(std::iter::once(0))
            .collect();
        let privileges = SERVICE_REQUIRED_PRIVILEGES_INFOW {
            pmszRequiredPrivileges: privileges.as_mut_ptr(),
        };
        // SAFETY: as above.
        unsafe {
            ChangeServiceConfig2W(
                service.0,
                SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO,
                &privileges as *const _ as _,
            );
        }
    }
}

/// Copies through a staging name, so a half-written file never sits under
/// the real one.
fn copy_replacing(source: &Path, target: &Path) -> Result<(), i32> {
    let staging = target.with_extension("new");
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

pub fn uninstall(spec: &ServiceSpec) -> Result<(), i32> {
    let manager = open_manager(SC_MANAGER_CONNECT).ok_or(exit::REMOVE_FAILED)?;
    match open_service(
        &manager,
        spec.name,
        SERVICE_STOP | SERVICE_QUERY_STATUS | DELETE,
    ) {
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
    remove_files(spec);
    Ok(())
}

fn remove_files(spec: &ServiceSpec) {
    let Some(dir) = spec.dir() else {
        return;
    };
    if std::fs::remove_dir_all(&dir).is_ok() {
        return;
    }
    // Still locked: leave it to the next reboot rather than fail the uninstall.
    let files = std::iter::once(spec.exe_name)
        .chain(std::iter::once(spec.log_name))
        .chain(spec.companions.iter().copied())
        .map(|name| dir.join(name));
    for path in files.chain(std::iter::once(dir.clone())) {
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
// From inside a service
// ---------------------------------------------------------------------------

/// Used once the app a service belongs to has been uninstalled: marks the
/// service for deletion (completed by the SCM when it stops) and its files
/// for removal at the next reboot, since a running image cannot delete itself.
pub fn remove_self(spec: &ServiceSpec) {
    if let Some(manager) = open_manager(SC_MANAGER_CONNECT) {
        if let Ok(service) = open_service(&manager, spec.name, DELETE) {
            // SAFETY: valid handle opened with DELETE.
            unsafe { DeleteService(service.0) };
        }
    }
    remove_files(spec);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: ServiceSpec = ServiceSpec {
        name: "OpenWireTestNoSuchService",
        display_name: "x",
        description: "x",
        dir_name: "OpenWire Test",
        exe_name: "openwire-test.exe",
        log_name: "test.log",
        service_flag: "--x",
        privileges: &[],
        dependencies: &[],
        companions: &[],
    };

    #[test]
    fn a_service_lives_in_its_own_folder_under_program_files() {
        let exe = SPEC.exe().expect("known folder resolves");
        assert!(exe.starts_with(program_files().unwrap()));
        assert!(exe.ends_with("OpenWire Test/openwire-test.exe"));
    }

    #[test]
    fn querying_a_missing_service_is_quiet() {
        assert_eq!(query(&SPEC), ServiceState::default());
    }

    #[test]
    fn a_companion_is_found_beside_the_exe_or_under_resources() {
        let root = std::env::temp_dir().join(format!("openwire-companion-{}", std::process::id()));
        let exe = root.join("app.exe");
        std::fs::create_dir_all(root.join("resources")).unwrap();

        assert_eq!(companion_source(&exe, "lib.dll"), None);
        std::fs::write(root.join("resources").join("lib.dll"), b"x").unwrap();
        assert_eq!(
            companion_source(&exe, "lib.dll"),
            Some(root.join("resources").join("lib.dll"))
        );
        std::fs::write(root.join("lib.dll"), b"x").unwrap();
        assert_eq!(
            companion_source(&exe, "lib.dll"),
            Some(root.join("lib.dll"))
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
