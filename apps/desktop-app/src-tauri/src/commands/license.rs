//! License commands.
//!
//! Both work before activation - the activation screen is built from them.

use tauri::State;

use crate::core::license::LicenseStatus;
use crate::error::AppResult;
use crate::state::AppState;

/// This machine's ID plus whether a valid key is active.
#[tauri::command]
pub async fn license_status(state: State<'_, AppState>) -> AppResult<LicenseStatus> {
    Ok(state.license_status())
}

/// Verifies a pasted key and stores it only if it is valid for this machine.
#[tauri::command]
pub async fn license_activate(
    state: State<'_, AppState>,
    key: String,
) -> AppResult<LicenseStatus> {
    state.activate_license(&key)
}
