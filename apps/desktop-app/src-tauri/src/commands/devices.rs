//! The Devices view: everything else on this network.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::core::lan::{Device, DeviceKind, DevicesSnapshot, Lan};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// The list changed: a sweep finished, or the user edited a device.
pub const EVENT_DEVICES: &str = "devices:changed";
/// A sweep is under way: `{ done, total }` addresses asked.
pub const EVENT_DEVICES_PROGRESS: &str = "devices:progress";

#[tauri::command]
pub async fn devices_snapshot(state: State<'_, AppState>) -> AppResult<DevicesSnapshot> {
    let lan = Arc::clone(&state.lan);
    tauri::async_runtime::spawn_blocking(move || {
        lan.refresh_network();
        lan.snapshot()
    })
    .await
    .map_err(|err| AppError::Internal(format!("device snapshot: {err}")))
}

#[tauri::command]
pub async fn devices_scan(app: AppHandle) -> AppResult<DevicesSnapshot> {
    scan(&app).await
}

#[tauri::command]
pub async fn devices_rename(
    app: AppHandle,
    mac: String,
    name: Option<String>,
) -> AppResult<DevicesSnapshot> {
    changed(&app, |lan| lan.rename(&mac, name))
}

/// `kind: null` returns the device to the automatic guess.
#[tauri::command]
pub async fn devices_set_kind(
    app: AppHandle,
    mac: String,
    kind: Option<DeviceKind>,
) -> AppResult<DevicesSnapshot> {
    changed(&app, |lan| lan.set_kind(&mac, kind))
}

/// `mac: null` marks every device as seen.
#[tauri::command]
pub async fn devices_acknowledge(app: AppHandle, mac: Option<String>) -> AppResult<DevicesSnapshot> {
    changed(&app, |lan| lan.acknowledge(mac.as_deref()))
}

#[tauri::command]
pub async fn devices_forget(app: AppHandle, mac: String) -> AppResult<DevicesSnapshot> {
    changed(&app, |lan| lan.forget(&mac))
}

/// Runs one sweep, telling every open view as it goes. Also what the
/// background watcher calls.
pub async fn scan(app: &AppHandle) -> AppResult<DevicesSnapshot> {
    let lan = Arc::clone(&app.state::<AppState>().lan);
    let emitter = app.clone();
    let outcome = lan
        .scan(move |progress| {
            let _ = emitter.emit(EVENT_DEVICES_PROGRESS, progress);
        })
        .await?;

    let _ = app.emit(EVENT_DEVICES, &outcome.snapshot);
    if let Some(message) = joined_message(&outcome.joined) {
        app.state::<AppState>().notify("info", message);
    }
    Ok(outcome.snapshot)
}

fn changed(app: &AppHandle, edit: impl FnOnce(&Lan) -> AppResult<()>) -> AppResult<DevicesSnapshot> {
    let lan = Arc::clone(&app.state::<AppState>().lan);
    edit(&lan)?;
    let snapshot = lan.snapshot();
    // The title bar's new-device count listens for this too.
    let _ = app.emit(EVENT_DEVICES, &snapshot);
    Ok(snapshot)
}

fn joined_message(joined: &[Device]) -> Option<String> {
    match joined {
        [] => None,
        [device] => {
            let label = device
                .name
                .clone()
                .or_else(|| device.vendor.clone().map(|vendor| format!("a {vendor} device")))
                .unwrap_or_else(|| format!("a device at {}", device.ip));
            Some(format!("New on your network: {label}. See it in Devices."))
        }
        many => Some(format!(
            "{} new devices joined your network. See them in Devices.",
            many.len()
        )),
    }
}
