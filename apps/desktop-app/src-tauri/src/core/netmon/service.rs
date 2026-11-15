//! The Windows service: trace, aggregate, and serve.
//!
//! Three moving parts, all stopped by one watch channel:
//! - a dedicated thread blocked in ETW, filling the shared [`Collector`];
//! - a one-second ticker draining it into rows, sent to connected clients or
//!   held in [`Pending`] while nobody is listening;
//! - the pipe server, accepting any number of read-only clients.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::net::windows::named_pipe::NamedPipeServer;
use tokio::sync::{broadcast, watch};

use super::etw::{self, Collector};
use super::flows::{self, Pending};
use super::sys;
use super::{Message, Row, PIPE_NAME, PROTOCOL, REVISION, SERVICE_NAME};
use crate::core::usage::unix_seconds;
use crate::core::winsvc::pipe::{self as winpipe, PipeSecurity};
use crate::core::winsvc::{self, host};

/// Lines buffered per client before a slow one starts skipping seconds.
const CLIENT_BACKLOG: usize = 64;
/// How often this machine's addresses are re-read; they change on roaming.
const LOCAL_REFRESH_TICKS: u64 = 30;
/// How often the service checks the app it belongs to still exists.
const OWNER_CHECK_TICKS: u64 = 3600;

/// Entry point for `--netmon-service`.
pub fn run() -> i32 {
    sys::init_service_log();
    host::run(host::Service {
        name: SERVICE_NAME,
        main,
        // Unblocks the ETW thread, which would otherwise sit in ProcessTrace.
        on_stop: etw::stop_by_name,
    })
}

fn main(stop: watch::Receiver<bool>) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            log::error!("could not build the runtime: {err}");
            return;
        }
    };
    runtime.block_on(serve(stop, winsvc::owner_from_args()));
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
                host::request_stop();
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
    let security = match PipeSecurity::new(winpipe::SDDL_READ_ONLY) {
        Ok(security) => security,
        Err(err) => {
            log::error!("pipe security descriptor: {err}");
            return;
        }
    };

    let mut first = true;
    loop {
        // Outbound only: the server never parses a byte a client sends.
        let server = match security.create(PIPE_NAME, first, winpipe::Direction::Outbound) {
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
    let pid = winpipe::client_pid(&pipe);
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
