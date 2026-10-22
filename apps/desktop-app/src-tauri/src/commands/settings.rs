//! Settings commands.

use tauri::image::Image;
use tauri::{AppHandle, Manager, State};

use crate::core::models::{ConnectionState, Settings, SettingsPatch};
use crate::core::sysproxy::{self, Recovery};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> AppResult<Settings> {
    Ok(state.settings())
}

#[tauri::command]
pub async fn settings_update(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> AppResult<Settings> {
    let previous = state.settings();
    let mut next = previous.clone();
    next.apply(patch);

    let saved = state.write_settings(next)?;

    // These three are baked into the live session at connect time. Rather than
    // silently doing nothing, say so.
    if state.is_tunnel_up()
        && (saved.listen_port != previous.listen_port
            || saved.lan_bypass != previous.lan_bypass
            || saved.kill_switch != previous.kill_switch)
    {
        state.notify("info", "Reconnect to apply the new connection settings.");
    }

    if saved.taskbar_speed != previous.taskbar_speed {
        crate::taskbar::set_enabled(&app, saved.taskbar_speed);
    }

    // Monitor mode is the one toggle that takes effect immediately, because it
    // only ever starts or stops a route that no VPN session is using.
    if saved.monitor_when_idle != previous.monitor_when_idle {
        if saved.monitor_when_idle {
            if !state.is_tunnel_up() {
                if let Err(err) = super::vpn::start_monitor(app, state.inner()).await {
                    state.notify("warning", err.user_message());
                }
            }
        } else if state.is_monitoring() {
            state.teardown();
            state.update_connection(|conn| *conn = ConnectionState::default());
        }
    }

    Ok(saved)
}

/// Platform capabilities, so the UI can explain what it can and cannot do here
/// instead of offering a toggle that would silently no-op.
#[tauri::command]
pub fn platform_info() -> serde_json::Value {
    serde_json::json!({
        "systemProxySupported": crate::core::sysproxy::is_supported(),
        "platform": std::env::consts::OS,
        "version": env!("CARGO_PKG_VERSION"),
    })
}

/// Matches the window and tray icons to the app's light or dark mode. The
/// icon baked into the executable (taskbar pin, Start menu, installer) stays
/// the dark one, which reads on either taskbar.
#[tauri::command]
pub fn app_icon_set(app: AppHandle, dark: bool) -> AppResult<()> {
    let bytes: &[u8] = if dark {
        include_bytes!("../../icons/128x128@2x.png")
    } else {
        include_bytes!("../../icons/light/256x256.png")
    };
    let icon = Image::from_bytes(bytes).map_err(|err| AppError::Internal(format!("app icon: {err}")))?;

    if let Some(window) = app.get_webview_window("main") {
        window
            .set_icon(icon.clone())
            .map_err(|err| AppError::Internal(format!("window icon: {err}")))?;
    }
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_icon(Some(icon))
            .map_err(|err| AppError::Internal(format!("tray icon: {err}")))?;
    }
    Ok(())
}

/// Manual escape hatch for a stranded system proxy.
///
/// The same recovery runs automatically at startup, but a user whose browsing
/// just died needs a button they can press now, without knowing that relaunching
/// would have fixed it.
#[tauri::command]
pub async fn network_restore(state: State<'_, AppState>) -> AppResult<String> {
    if state.is_monitoring() {
        return Err(AppError::Invalid(
            "Turn off monitor mode first - it is using the proxy right now.".to_string(),
        ));
    }
    if state.is_tunnel_up() {
        return Err(AppError::Invalid(
            "Disconnect first - the proxy is in use right now.".to_string(),
        ));
    }

    let outcome = sysproxy::recover(&state.proxy_backup_path())?;
    Ok(match outcome {
        Recovery::Restored => "Your previous network settings are back.".to_string(),
        Recovery::Cleared => "Cleared a leftover proxy. Networking should work again.".to_string(),
        Recovery::NothingToDo => "Your network settings are already clean.".to_string(),
    })
}
