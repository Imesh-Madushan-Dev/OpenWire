//! Settings commands.

use tauri::State;

use crate::core::models::{Settings, SettingsPatch};
use crate::core::sysproxy::{self, Recovery};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> AppResult<Settings> {
    Ok(state.settings())
}

#[tauri::command]
pub async fn settings_update(
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

/// Manual escape hatch for a stranded system proxy.
///
/// The same recovery runs automatically at startup, but a user whose browsing
/// just died needs a button they can press now, without knowing that relaunching
/// would have fixed it.
#[tauri::command]
pub async fn network_restore(state: State<'_, AppState>) -> AppResult<String> {
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
