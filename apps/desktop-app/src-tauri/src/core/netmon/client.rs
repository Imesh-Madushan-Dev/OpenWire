//! The app's side of the monitor: watch the service, read its pipe, and hand
//! what arrives to usage history.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::Notify;

use crate::core::usage::UsageTracker;

/// What the settings page and the Usage view need to know.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorStatus {
    /// False off Windows, where there is nothing to install.
    pub supported: bool,
    pub installed: bool,
    pub running: bool,
    /// The app is receiving the service's stream right now.
    pub connected: bool,
    /// The service's trace session is up.
    pub collecting: bool,
    /// Why it is not collecting, in words fit for the UI.
    pub error: Option<String>,
    /// The installed service speaks a different protocol than this build and
    /// needs reinstalling - which happens after an app update.
    pub outdated: bool,
}

pub struct Monitor {
    status: Mutex<MonitorStatus>,
    /// Cuts the reconnect wait short, e.g. right after an install.
    wake: Notify,
}

impl Monitor {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            status: Mutex::new(MonitorStatus {
                supported: cfg!(windows),
                ..MonitorStatus::default()
            }),
            wake: Notify::new(),
        })
    }

    pub fn status(&self) -> MonitorStatus {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_default()
    }

    pub fn wake(&self) {
        self.wake.notify_one();
    }

    /// Applies `change`, reporting the new status if anything moved.
    fn update(&self, change: impl FnOnce(&mut MonitorStatus)) -> Option<MonitorStatus> {
        let mut status = self.status.lock().ok()?;
        let before = status.clone();
        change(&mut status);
        (*status != before).then(|| status.clone())
    }

    /// Runs for the life of the app, reconnecting whenever the service comes
    /// and goes. `notify` hears every status change.
    pub fn spawn(
        self: &Arc<Self>,
        usage: Arc<UsageTracker>,
        notify: impl Fn(MonitorStatus) + Send + Sync + 'static,
    ) {
        #[cfg(windows)]
        {
            let monitor = Arc::clone(self);
            tauri::async_runtime::spawn(async move {
                windows::run(monitor, usage, notify).await;
            });
        }
        #[cfg(not(windows))]
        {
            let _ = (usage, notify);
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

    use super::super::{scm, Message, PIPE_NAME, PROTOCOL};
    use super::{Monitor, MonitorStatus};
    use crate::core::usage::UsageTracker;

    /// How often to look for the service while it is absent. Each look is one
    /// SCM query, so this costs nothing worth measuring.
    const POLL: Duration = Duration::from_secs(5);
    /// No legitimate line comes near this; a stream that does is broken.
    const MAX_LINE: u64 = 8 * 1024 * 1024;

    pub async fn run(
        monitor: Arc<Monitor>,
        usage: Arc<UsageTracker>,
        notify: impl Fn(MonitorStatus),
    ) {
        let report = |change: &dyn Fn(&mut MonitorStatus)| {
            if let Some(status) = monitor.update(|status| change(status)) {
                notify(status);
            }
        };

        // A service found to speak another protocol is left alone until it is
        // replaced, rather than reconnected to (and rejected) every poll.
        let mut outdated_pid = None;

        loop {
            let service = tokio::task::spawn_blocking(scm::query)
                .await
                .unwrap_or_default();
            let outdated = service.running && outdated_pid == Some(service.pid);
            report(&|status| {
                status.installed = service.installed;
                status.running = service.running;
                status.outdated = outdated;
                if !service.running {
                    status.connected = false;
                    status.collecting = false;
                }
            });

            if service.running && !outdated {
                if let Some(pipe) = connect(service.pid).await {
                    report(&|status| status.connected = true);
                    if read(pipe, &usage, &report).await == Ended::Outdated {
                        outdated_pid = Some(service.pid);
                    }
                    report(&|status| {
                        status.connected = false;
                        status.collecting = false;
                    });
                }
            }

            tokio::select! {
                _ = tokio::time::sleep(POLL) => {}
                _ = monitor.wake.notified() => {}
            }
        }
    }

    /// Opens the pipe read-only, and only keeps it if the process serving it
    /// is the service the SCM says is running. Anyone can create a pipe by
    /// that name before the service does; this is what makes that pointless.
    async fn connect(service_pid: u32) -> Option<NamedPipeClient> {
        for _ in 0..20 {
            match ClientOptions::new().read(true).write(false).open(PIPE_NAME) {
                Ok(pipe) => {
                    return (server_pid(&pipe) == Some(service_pid))
                        .then_some(pipe)
                        .or_else(|| {
                            log::warn!("network monitor pipe is served by an unexpected process");
                            None
                        });
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

    fn server_pid(pipe: &NamedPipeClient) -> Option<u32> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId;

        let mut pid = 0u32;
        // SAFETY: a live pipe handle and a valid out-pointer.
        let ok = unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle() as _, &mut pid) };
        (ok != 0).then_some(pid)
    }

    #[derive(PartialEq, Eq)]
    enum Ended {
        Closed,
        Outdated,
    }

    async fn read(
        pipe: NamedPipeClient,
        usage: &UsageTracker,
        report: &impl Fn(&dyn Fn(&mut MonitorStatus)),
    ) -> Ended {
        let mut reader = BufReader::new(pipe);
        let mut line = Vec::new();

        loop {
            line.clear();
            match (&mut reader)
                .take(MAX_LINE)
                .read_until(b'\n', &mut line)
                .await
            {
                Ok(0) | Err(_) => return Ended::Closed,
                Ok(_) if line.last() != Some(&b'\n') => {
                    log::warn!("network monitor sent an oversized line; reconnecting");
                    return Ended::Closed;
                }
                Ok(_) => {}
            }

            let Ok(message) = serde_json::from_slice::<Message>(&line) else {
                // A newer service may send kinds of message this build does
                // not know; skipping them is the forward-compatible choice.
                continue;
            };

            match message {
                Message::Hello { protocol, .. } => {
                    if protocol != PROTOCOL {
                        report(&|status| status.outdated = true);
                        return Ended::Outdated;
                    }
                }
                Message::Status { collecting, error } => {
                    report(&|status| {
                        status.collecting = collecting;
                        status.error = error.clone();
                    });
                }
                Message::Usage { rows } => usage.record_monitored(&rows),
            }
        }
    }
}
