//! System-wide routing: the tunnel service that carries every app.

use crate::error::{AppError, AppResult};

#[cfg(windows)]
pub use crate::core::tun::client::ServiceStatus;

#[cfg(not(windows))]
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    pub supported: bool,
    pub installed: bool,
    pub running: bool,
    pub problem: Option<String>,
}

/// Whether the service is there and would take this app's requests.
#[tauri::command]
pub async fn tun_status() -> AppResult<ServiceStatus> {
    #[cfg(windows)]
    {
        Ok(crate::core::tun::client::status().await)
    }
    #[cfg(not(windows))]
    {
        Ok(ServiceStatus::default())
    }
}

/// Installs the tunnel service, or updates or repairs it in place. Windows
/// asks for admin approval first.
#[tauri::command]
pub async fn tun_install() -> AppResult<ServiceStatus> {
    change(true).await
}

/// Stops and removes the tunnel service.
#[tauri::command]
pub async fn tun_uninstall() -> AppResult<ServiceStatus> {
    change(false).await
}

#[cfg(windows)]
async fn change(install: bool) -> AppResult<ServiceStatus> {
    use crate::core::tun::{FLAG_INSTALL, FLAG_UNINSTALL};
    use crate::core::winsvc::elevate;

    let flag = if install {
        FLAG_INSTALL
    } else {
        FLAG_UNINSTALL
    };
    // Blocking for as long as the UAC prompt sits on screen.
    tauri::async_runtime::spawn_blocking(move || elevate(flag))
        .await
        .map_err(|err| AppError::Internal(format!("elevation worker: {err}")))?
        .map_err(|err| AppError::Tun(describe(err, install)))?;

    Ok(crate::core::tun::client::status().await)
}

#[cfg(not(windows))]
async fn change(_install: bool) -> AppResult<ServiceStatus> {
    Err(AppError::Tun(
        "System-wide routing is only available on Windows.".to_string(),
    ))
}

#[cfg(windows)]
fn describe(err: crate::core::winsvc::ElevationError, install: bool) -> String {
    use crate::core::winsvc::{exit, ElevationError};

    match err {
        ElevationError::Declined if install => {
            "Windows didn't get admin approval, so system-wide routing stays off.".into()
        }
        ElevationError::Declined => {
            "Windows didn't get admin approval, so the tunnel service is still installed.".into()
        }
        ElevationError::Launch => "Couldn't ask Windows for admin approval.".into(),
        ElevationError::TimedOut => "Windows took too long to respond. Try again.".into(),
        ElevationError::Failed(exit::NO_PROGRAM_FILES | exit::COPY_FAILED) => {
            "Couldn't copy the tunnel service into Program Files.".into()
        }
        ElevationError::Failed(exit::COMPANION_MISSING) => {
            "The network driver is missing from OpenWire's folder. Reinstall OpenWire.".into()
        }
        ElevationError::Failed(exit::REGISTER_FAILED) => {
            "Windows wouldn't register the tunnel service.".into()
        }
        ElevationError::Failed(exit::START_FAILED) => {
            "The tunnel service was installed but wouldn't start. Restarting your PC usually fixes this."
                .into()
        }
        ElevationError::Failed(exit::REMOVE_FAILED) => {
            "Windows wouldn't remove the tunnel service.".into()
        }
        ElevationError::Failed(_) => "Setting up the tunnel service failed. Try again.".into(),
    }
}
