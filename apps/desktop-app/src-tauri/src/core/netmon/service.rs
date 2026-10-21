//! The Windows service: trace, aggregate, and serve.
//!
//! Three moving parts, all stopped by one watch channel:
//! - a dedicated thread blocked in ETW, filling the shared [`Collector`];
//! - a one-second ticker draining it into rows, sent to connected clients or
//!   held in [`Pending`] while nobody is listening;
//! - the pipe server, accepting any number of read-only clients.

use std::collections::HashSet;
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::{broadcast, watch};
use windows_sys::Win32::Foundation::{ERROR_CALL_NOT_IMPLEMENTED, NO_ERROR};
use windows_sys::Win32::System::Services::{
    RegisterServiceCtrlHandlerExW, SetServiceStatus, StartServiceCtrlDispatcherW,
    SERVICE_ACCEPT_SHUTDOWN, SERVICE_ACCEPT_STOP, SERVICE_CONTROL_INTERROGATE,
    SERVICE_CONTROL_SHUTDOWN, SERVICE_CONTROL_STOP, SERVICE_RUNNING, SERVICE_START_PENDING,
    SERVICE_STATUS, SERVICE_STATUS_HANDLE, SERVICE_STOPPED, SERVICE_STOP_PENDING,
    SERVICE_TABLE_ENTRYW, SERVICE_WIN32_OWN_PROCESS,
};

use super::etw::{self, Collector};
use super::flows::{self, Pending};
use super::sys::{self, wide};
use super::{Message, Row, FLAG_OWNER, PIPE_NAME, PROTOCOL, REVISION, SERVICE_NAME};
use crate::core::usage::unix_seconds;

/// Lines buffered per client before a slow one starts skipping seconds.
const CLIENT_BACKLOG: usize = 64;
/// How often this machine's addresses are re-read; they change on roaming.
const LOCAL_REFRESH_TICKS: u64 = 30;
/// How often the service checks the app it belongs to still exists.
const OWNER_CHECK_TICKS: u64 = 3600;

/// Process-wide: the SCM calls back into plain functions, not closures.
static STATUS_HANDLE: OnceLock<usize> = OnceLock::new();
static STOP: OnceLock<watch::Sender<bool>> = OnceLock::new();

/// Entry point for `--netmon-service`. Hands the thread to the SCM, which
/// calls `service_main` on another and returns here once the service stops.
pub fn run() -> i32 {
    sys::init_service_log();

    let mut name = wide(SERVICE_NAME);
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

unsafe extern "system" fn service_main(_argc: u32, _argv: *mut windows_sys::core::PWSTR) {
    let name = wide(SERVICE_NAME);
    let handle = RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(on_control), std::ptr::null());
    if handle.is_null() {
        log::error!("could not register the control handler");
        return;
    }
    let _ = STATUS_HANDLE.set(handle as usize);
    report(SERVICE_START_PENDING, 0);

    let (stop_tx, stop_rx) = watch::channel(false);
    let _ = STOP.set(stop_tx);

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            log::error!("could not build the runtime: {err}");
            report(SERVICE_STOPPED, 1);
            return;
        }
    };

    report(SERVICE_RUNNING, 0);
    log::info!("started");
    runtime.block_on(serve(stop_rx, owner_from_args()));
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

fn request_stop() {
    if let Some(stop) = STOP.get() {
        let _ = stop.send(true);
    }
    // Unblocks the ETW thread, which would otherwise sit in ProcessTrace.
    etw::stop_by_name();
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

fn owner_from_args() -> Option<PathBuf> {
    let mut args = std::env::args().skip_while(|arg| arg != FLAG_OWNER).skip(1);
    args.next().map(PathBuf::from)
}

// ---------------------------------------------------------------------------

/// Everything the tasks share.
struct Shared {
    pending: Mutex<Pending>,
    /// Pids of connected apps, whose traffic the relay already accounts for.
    clients: Mutex<HashSet<u32>>,
    /// The last status sent, replayed to each new client.
    status: Mutex<Message>,
    lines: broadcast::Sender<Arc<str>>,
}

impl Shared {
    fn set_status(&self, collecting: bool, error: Option<String>) {
        let message = Message::Status { collecting, error };
        let changed = self
            .status
            .lock()
            .map(|mut status| {
                let changed = *status != message;
                *status = message.clone();
                changed
            })
            .unwrap_or(false);
        if changed {
            if let Some(line) = encode(&message) {
                let _ = self.lines.send(line);
            }
        }
    }
}

async fn serve(stop: watch::Receiver<bool>, owner: Option<PathBuf>) {
    let collector = Arc::new(Collector::default());
    let (lines, _) = broadcast::channel(CLIENT_BACKLOG);
    let shared = Arc::new(Shared {
        pending: Mutex::new(Pending::default()),
        clients: Mutex::new(HashSet::new()),
        status: Mutex::new(Message::Status {
            collecting: false,
            error: None,
        }),
        lines,
    });

    let tracer = {
        let collector = Arc::clone(&collector);
        let shared = Arc::clone(&shared);
        let stop = stop.clone();
        std::thread::Builder::new()
            .name("netmon-etw".into())
            .spawn(move || trace_loop(&collector, &shared, &stop))
    };
    if let Err(err) = &tracer {
        log::error!("could not start the trace thread: {err}");
    }

    tokio::join!(
        tick_loop(
            Arc::clone(&collector),
            Arc::clone(&shared),
            stop.clone(),
            owner
        ),
        accept_loop(Arc::clone(&shared), stop.clone()),
    );

    if let Ok(thread) = tracer {
        let _ = thread.join();
    }
}

/// Keeps a trace session running, restarting it if Windows ends it.
fn trace_loop(collector: &Collector, shared: &Shared, stop: &watch::Receiver<bool>) {
    while !*stop.borrow() {
        let outcome = etw::Session::start().map(|session| {
            shared.set_status(true, None);
            log::info!("tracing");
            let result = etw::consume(collector);
            drop(session);
            result
        });

        if *stop.borrow() {
            break;
        }

        let (error, wait) = match outcome {
            Ok(Ok(())) => (
                "Network tracing ended unexpectedly; restarting.".to_string(),
                5,
            ),
            Ok(Err(err)) | Err(err) => (describe(&err), 30),
        };
        log::warn!("{error}");
        shared.set_status(false, Some(error));

        // Interruptible wait: a stop must not sit out the back-off.
        for _ in 0..wait * 10 {
            if *stop.borrow() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn describe(err: &std::io::Error) -> String {
    match err.raw_os_error() {
        Some(5) => "Windows refused network tracing (access denied).".into(),
        Some(1450) => {
            "Windows has no trace sessions left. Close other network monitoring tools.".into()
        }
        _ => format!("Windows could not start network tracing ({err})."),
    }
}

async fn tick_loop(
    collector: Arc<Collector>,
    shared: Arc<Shared>,
    mut stop: watch::Receiver<bool>,
    owner: Option<PathBuf>,
) {
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut local = sys::local_addresses();
    let mut owner_missing = false;
    let mut ticks: u64 = 0;

    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            _ = stop.changed() => return,
        }
        ticks += 1;

        if ticks % LOCAL_REFRESH_TICKS == 0 {
            local = sys::local_addresses();
        }

        if ticks % OWNER_CHECK_TICKS == 0 {
            // Twice in a row, an hour apart: an app update swaps the file for
            // a moment, and that must not read as an uninstall.
            let missing = owner.as_ref().is_some_and(|path| !path.exists());
            if missing && owner_missing {
                log::info!("the app is gone; removing the monitor");
                super::scm::remove_self();
                request_stop();
                return;
            }
            owner_missing = missing;
        }

        let drained = match collector.flows.lock() {
            Ok(mut flows) => flows.drain(),
            Err(_) => continue,
        };
        if drained.is_empty() {
            continue;
        }

        let excluded = shared.clients.lock().map(|c| c.clone()).unwrap_or_default();
        let rows = {
            let Ok(mut dns) = collector.dns.lock() else {
                continue;
            };
            flows::fold(drained, unix_seconds(), &excluded, &local, |ip| {
                dns.lookup(ip)
            })
        };
        if rows.is_empty() {
            continue;
        }

        // With nobody listening, hold the rows for whoever connects next.
        if shared.lines.receiver_count() == 0 {
            if let Ok(mut pending) = shared.pending.lock() {
                pending.add(rows);
            }
        } else if let Some(line) = encode(&Message::Usage { rows }) {
            let _ = shared.lines.send(line);
        }
    }
}

async fn accept_loop(shared: Arc<Shared>, mut stop: watch::Receiver<bool>) {
    let security = match PipeSecurity::new() {
        Ok(security) => security,
        Err(err) => {
            log::error!("pipe security descriptor: {err}");
            return;
        }
    };

    let mut first = true;
    loop {
        let server = match security.create(first) {
            Ok(server) => server,
            Err(err) => {
                // Most likely another process squatting on the name.
                log::error!("could not create the pipe: {err}");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(10)) => continue,
                    _ = stop.changed() => return,
                }
            }
        };
        first = false;

        tokio::select! {
            connected = server.connect() => {
                if connected.is_ok() {
                    tokio::spawn(serve_client(server, Arc::clone(&shared), stop.clone()));
                }
            }
            _ = stop.changed() => return,
        }
    }
}

async fn serve_client(
    mut pipe: NamedPipeServer,
    shared: Arc<Shared>,
    mut stop: watch::Receiver<bool>,
) {
    let pid = client_pid(&pipe);
    if let (Some(pid), Ok(mut clients)) = (pid, shared.clients.lock()) {
        clients.insert(pid);
    }

    // Subscribe before taking the backlog, so no second falls between them.
    let mut lines = shared.lines.subscribe();
    let backlog = shared
        .pending
        .lock()
        .map(|mut pending| pending.take())
        .unwrap_or_default();
    let status = shared.status.lock().map(|status| status.clone()).ok();

    let mut opening = vec![Message::Hello {
        protocol: PROTOCOL,
        version: env!("CARGO_PKG_VERSION").to_string(),
        revision: REVISION,
    }];
    opening.extend(status);
    if !backlog.is_empty() {
        opening.extend(backlog_messages(backlog));
    }

    let mut alive = true;
    for message in opening {
        if let Some(line) = encode(&message) {
            if pipe.write_all(line.as_bytes()).await.is_err() {
                alive = false;
                break;
            }
        }
    }

    while alive {
        tokio::select! {
            line = lines.recv() => match line {
                Ok(line) => alive = pipe.write_all(line.as_bytes()).await.is_ok(),
                // A client too slow to keep up loses seconds, not the service.
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    log::warn!("client {pid:?} skipped {skipped} updates");
                }
                Err(broadcast::error::RecvError::Closed) => alive = false,
            },
            _ = stop.changed() => alive = false,
        }
    }

    if let (Some(pid), Ok(mut clients)) = (pid, shared.clients.lock()) {
        clients.remove(&pid);
    }
}

/// Splits a long backlog so no single line grows unreasonably large.
fn backlog_messages(rows: Vec<Row>) -> Vec<Message> {
    rows.chunks(2_000)
        .map(|chunk| Message::Usage {
            rows: chunk.to_vec(),
        })
        .collect()
}

fn encode(message: &Message) -> Option<Arc<str>> {
    let mut line = serde_json::to_string(message).ok()?;
    line.push('\n');
    Some(Arc::from(line))
}

fn client_pid(pipe: &NamedPipeServer) -> Option<u32> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Pipes::GetNamedPipeClientProcessId;

    let mut pid = 0u32;
    // SAFETY: a live pipe handle and a valid out-pointer.
    let ok = unsafe { GetNamedPipeClientProcessId(pipe.as_raw_handle() as _, &mut pid) };
    (ok != 0).then_some(pid)
}

/// The pipe's DACL: SYSTEM and administrators in full, interactive users
/// read-only. The server is outbound-only on top of that, so a client could
/// not send anything even if it tried.
struct PipeSecurity {
    descriptor: windows_sys::Win32::Security::PSECURITY_DESCRIPTOR,
}

// SAFETY: the descriptor is immutable after creation and only read by
// CreateNamedPipeW, which may be called from any thread.
unsafe impl Send for PipeSecurity {}
unsafe impl Sync for PipeSecurity {}

impl PipeSecurity {
    fn new() -> std::io::Result<Self> {
        use windows_sys::Win32::Security::Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        };

        let sddl = wide("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GR;;;IU)");
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: the API allocates the descriptor with LocalAlloc; it is
        // freed in Drop.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self { descriptor })
    }

    fn create(&self, first: bool) -> std::io::Result<NamedPipeServer> {
        use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;

        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.descriptor,
            bInheritHandle: 0,
        };

        // SAFETY: the attributes point at a descriptor that outlives the call.
        unsafe {
            ServerOptions::new()
                // Refuse to start if someone else already owns the name, so a
                // squatter cannot sit between the service and the app.
                .first_pipe_instance(first)
                .access_inbound(false)
                .access_outbound(true)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(
                    PIPE_NAME,
                    &mut attributes as *mut SECURITY_ATTRIBUTES as *mut c_void,
                )
        }
    }
}

impl Drop for PipeSecurity {
    fn drop(&mut self) {
        // SAFETY: allocated by ConvertStringSecurityDescriptorToSecurityDescriptorW.
        unsafe { windows_sys::Win32::Foundation::LocalFree(self.descriptor as _) };
    }
}
