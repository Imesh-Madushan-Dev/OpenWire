//! Detailed traffic monitoring: the opt-in service that sees the whole device.

use tauri::{AppHandle, Manager, State};

use crate::core::netmon::client::MonitorStatus;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[tauri::command]
pub async fn netmon_status(state: State<'_, AppState>) -> AppResult<MonitorStatus> {
    Ok(state.netmon.status())
}

/// Installs the monitor service, or upgrades it in place. Windows asks the
/// user for admin approval first.
#[tauri::command]
pub async fn netmon_enable(app: AppHandle) -> AppResult<MonitorStatus> {
    change(app, true).await
}

/// Stops and removes the monitor service. Recorded history is kept.
#[tauri::command]
pub async fn netmon_disable(app: AppHandle) -> AppResult<MonitorStatus> {
    change(app, false).await
}

#[cfg(windows)]
async fn change(app: AppHandle, install: bool) -> AppResult<MonitorStatus> {
    use crate::core::netmon::{scm, FLAG_INSTALL, FLAG_UNINSTALL};

    let flag = if install {
        FLAG_INSTALL
    } else {
        FLAG_UNINSTALL
    };
    // Blocking for as long as the UAC prompt sits on screen.
    tauri::async_runtime::spawn_blocking(move || scm::elevate(flag))
        .await
        .map_err(|err| AppError::Internal(format!("elevation worker: {err}")))?
        .map_err(|err| AppError::Monitor(describe(err, install)))?;

    let state = app.state::<AppState>();
    state.netmon.wake();

    // The watcher catches up within a moment and reports the rest by event;
    // answering with the SCM's view now keeps the switch from snapping back.
    let service = tauri::async_runtime::spawn_blocking(scm::query)
        .await
        .unwrap_or_default();
    let mut status = state.netmon.status();
    status.installed = service.installed;
    status.running = service.running;
    if !service.running {
        status.connected = false;
        status.collecting = false;
    }
    Ok(status)
}

#[cfg(not(windows))]
async fn change(_app: AppHandle, _install: bool) -> AppResult<MonitorStatus> {
    Err(AppError::Monitor(
        "Detailed monitoring is only available on Windows.".to_string(),
    ))
}

#[cfg(windows)]
fn describe(err: crate::core::netmon::scm::ElevationError, install: bool) -> String {
    use crate::core::netmon::scm::{exit, ElevationError};

    match err {
        ElevationError::Declined if install => {
            "Windows didn't get admin approval, so detailed monitoring stays off.".into()
        }
        ElevationError::Declined => {
            "Windows didn't get admin approval, so the monitor is still installed.".into()
        }
        ElevationError::Launch => "Couldn't ask Windows for admin approval.".into(),
        ElevationError::TimedOut => "Windows took too long to respond. Try again.".into(),
        ElevationError::Failed(exit::NO_PROGRAM_FILES | exit::COPY_FAILED) => {
            "Couldn't copy the monitor into Program Files.".into()
        }
        ElevationError::Failed(exit::REGISTER_FAILED) => {
            "Windows wouldn't register the monitor service.".into()
        }
        ElevationError::Failed(exit::START_FAILED) => {
            "The monitor was installed but wouldn't start. Restarting your PC usually fixes this."
                .into()
        }
        ElevationError::Failed(exit::REMOVE_FAILED) => {
            "Windows wouldn't remove the monitor service.".into()
        }
        ElevationError::Failed(_) => "Setting up the monitor failed. Try again.".into(),
    }
}
