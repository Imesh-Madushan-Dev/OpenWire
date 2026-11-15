//! The app's side of the tunnel service: start a routing session, stop it,
//! and hear if it ends on its own.
//!
//! A [`TunSession`] owns the pipe through a task of its own. Dropping it
//! closes the pipe, which the service takes as the end of the session - so
//! like [`crate::core::sysproxy::ProxyGuard`], forgetting to stop it cannot
//! leave the machine routed at a proxy that has gone.

use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, WriteHalf};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

use super::{scm, Event, Request, StartRequest, PIPE_NAME, PROTOCOL};
use crate::core::winsvc::pipe as winpipe;

/// Creating the adapter is the slow part: a second or two, longer the very
/// first time while Windows installs the driver.
const START_TIMEOUT: Duration = Duration::from_secs(60);
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const STOP_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_LINE: u64 = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TunError {
    NotInstalled,
    NotRunning,
    /// The service speaks an older protocol; it needs updating, which an app
    /// update normally does.
    Outdated,
    /// The service said no, in words fit for the UI.
    Refused(String),
    /// The pipe broke or the service answered nonsense.
    Broken,
}

impl TunError {
    pub fn user_message(&self) -> String {
        match self {
            Self::NotInstalled => {
                "System-wide routing needs the OpenWire Tunnel service. Turn it on in Settings > Routing."
                    .into()
            }
            Self::NotRunning => {
                "The OpenWire Tunnel service isn't running. Repair it in Settings > Routing.".into()
            }
            Self::Outdated => {
                "The OpenWire Tunnel service is older than this app. Update it in Settings > Routing."
                    .into()
            }
            Self::Refused(message) => message.clone(),
            Self::Broken => {
                "Couldn't talk to the OpenWire Tunnel service. Repair it in Settings > Routing.".into()
            }
        }
    }
}

/// What Settings shows about the service.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    pub supported: bool,
    pub installed: bool,
    pub running: bool,
    /// Why it can't be used as it is, in words fit for the UI.
    pub problem: Option<String>,
}

/// Asks the service whether it would take this app's requests, without
/// starting anything.
pub async fn status() -> ServiceStatus {
    let service = tokio::task::spawn_blocking(scm::query)
        .await
        .unwrap_or_default();
    let mut status = ServiceStatus {
        supported: true,
        installed: service.installed,
        running: service.running,
        problem: None,
    };
    if service.running {
        status.problem = match handshake(service.pid).await {
            Ok(_) => None,
            Err(err) => Some(err.user_message()),
        };
    }
    status
}

/// The pipe, said hello to.
struct Connection {
    writer: WriteHalf<NamedPipeClient>,
    events: mpsc::Receiver<Event>,
    /// The task holding the read half. The pipe only closes once both
    /// halves are gone, and that task would otherwise sit in a read the
    /// service never answers.
    reader: AbortOnDrop,
}

struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn handshake(service_pid: u32) -> Result<Connection, TunError> {
    let pipe = connect(service_pid).await.ok_or(TunError::NotRunning)?;
    let (reader, writer) = tokio::io::split(pipe);
    let (mut events, reader) = read_events(reader);
    match timeout(HELLO_TIMEOUT, events.recv()).await {
        Ok(Some(Event::Hello { protocol, .. })) if protocol == PROTOCOL => Ok(Connection {
            writer,
            events,
            reader,
        }),
        Ok(Some(Event::Hello { .. })) => Err(TunError::Outdated),
        Ok(Some(Event::Error { message })) => Err(TunError::Refused(message)),
        _ => Err(TunError::Broken),
    }
}

/// Opens the pipe, and only keeps it if the process serving it is the
/// service the SCM says is running. Anyone can create a pipe by that name
/// first; this is what makes that pointless.
async fn connect(service_pid: u32) -> Option<NamedPipeClient> {
    for _ in 0..20 {
        match ClientOptions::new().open(PIPE_NAME) {
            Ok(pipe) => {
                if winpipe::server_pid(&pipe) == Some(service_pid) {
                    return Some(pipe);
                }
                log::warn!("tunnel service pipe is served by an unexpected process");
                return None;
            }
            // Every instance is mid-handshake with someone; try again shortly.
            Err(err) if err.raw_os_error() == Some(231) => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(_) => return None,
        }
    }
    None
}

fn read_events(
    reader: tokio::io::ReadHalf<NamedPipeClient>,
) -> (mpsc::Receiver<Event>, AbortOnDrop) {
    let (tx, rx) = mpsc::channel(8);
    let task = tokio::spawn(async move {
        let mut reader = BufReader::new(reader);
        let mut line = Vec::new();
        loop {
            line.clear();
            match (&mut reader)
                .take(MAX_LINE)
                .read_until(b'\n', &mut line)
                .await
            {
                Ok(0) | Err(_) => break,
                Ok(_) if line.last() != Some(&b'\n') => break,
                Ok(_) => {}
            }
            // A newer service may send events this build does not know;
            // skipping them is the forward-compatible choice.
            if let Ok(event) = serde_json::from_slice::<Event>(&line) {
                if tx.send(event).await.is_err() {
                    break;
                }
            }
        }
    });
    (rx, AbortOnDrop(task))
}

async fn send(writer: &mut WriteHalf<NamedPipeClient>, request: &Request) -> Result<(), TunError> {
    let mut line = serde_json::to_vec(request).map_err(|_| TunError::Broken)?;
    line.push(b'\n');
    writer
        .write_all(&line)
        .await
        .map_err(|_| TunError::Broken)?;
    writer.flush().await.map_err(|_| TunError::Broken)
}

/// A routing session. Dropping it ends the session.
pub struct TunSession {
    stop: Option<oneshot::Sender<oneshot::Sender<()>>>,
    /// The adapter's interface index, which the app's own sockets avoid.
    pub interface_index: u32,
    pub ipv6: bool,
}

impl TunSession {
    /// Starts routing. `lost` hears, once, if the session later ends without
    /// being asked to - with why, in words fit for the UI.
    pub async fn start(
        request: StartRequest,
        lost: impl FnOnce(String) + Send + 'static,
    ) -> Result<Self, TunError> {
        let service = tokio::task::spawn_blocking(scm::query)
            .await
            .unwrap_or_default();
        if !service.installed {
            return Err(TunError::NotInstalled);
        }
        if !service.running {
            return Err(TunError::NotRunning);
        }

        let Connection {
            mut writer,
            mut events,
            reader,
        } = handshake(service.pid).await?;
        send(&mut writer, &Request::Start(request)).await?;

        let (interface_index, ipv6) = match timeout(START_TIMEOUT, events.recv()).await {
            Ok(Some(Event::Started {
                interface_index,
                ipv6,
            })) => (interface_index, ipv6),
            Ok(Some(Event::Error { message })) => return Err(TunError::Refused(message)),
            _ => return Err(TunError::Broken),
        };

        let (stop, stop_requested) = oneshot::channel::<oneshot::Sender<()>>();
        tokio::spawn(hold(writer, events, reader, stop_requested, lost));

        Ok(Self {
            stop: Some(stop),
            interface_index,
            ipv6,
        })
    }

    /// Ends the session and waits for the service to confirm the routes are
    /// gone - so a caller can take the proxy down right after, knowing
    /// nothing is pointed at it any more.
    pub async fn stop(mut self) {
        if let Some(stop) = self.stop.take() {
            let (done, confirmed) = oneshot::channel();
            if stop.send(done).is_ok() {
                let _ = timeout(STOP_TIMEOUT, confirmed).await;
            }
        }
    }
}

/// Owns the pipe for the life of the session; returning closes it.
async fn hold(
    mut writer: WriteHalf<NamedPipeClient>,
    mut events: mpsc::Receiver<Event>,
    _reader: AbortOnDrop,
    stop_requested: oneshot::Receiver<oneshot::Sender<()>>,
    lost: impl FnOnce(String),
) {
    tokio::select! {
        requested = stop_requested => {
            if let Ok(done) = requested {
                if send(&mut writer, &Request::Stop).await.is_ok() {
                    let _ = timeout(STOP_TIMEOUT, async {
                        while let Some(event) = events.recv().await {
                            if matches!(event, Event::Stopped { .. }) {
                                break;
                            }
                        }
                    })
                    .await;
                }
                let _ = done.send(());
            }
            // Dropped without `stop`: closing the pipe below ends it anyway.
        }
        reason = async {
            while let Some(event) = events.recv().await {
                if let Event::Stopped { reason } = event {
                    return reason.unwrap_or_else(|| "System-wide routing stopped.".into());
                }
            }
            "The OpenWire Tunnel service stopped.".to_string()
        } => lost(reason),
    }
}
