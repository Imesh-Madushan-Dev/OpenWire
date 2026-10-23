//! Release check commands.

use tauri::State;

use crate::core::models::UpdateStatus;
use crate::core::update;
use crate::error::AppResult;
use crate::state::{now_millis, AppState};

/// What the last check found, without asking GitHub again.
#[tauri::command]
pub fn update_status(state: State<'_, AppState>) -> UpdateStatus {
    state.update_status()
}

/// Asks GitHub now, whether or not automatic checks are on: the user asked.
#[tauri::command]
pub async fn update_check(state: State<'_, AppState>) -> AppResult<UpdateStatus> {
    check(&state).await.map(|(status, _)| status)
}

/// Asks GitHub for the newest release and records the answer. Also returns
/// the newer version the first time it is found, for the background check to
/// announce.
pub async fn check(state: &AppState) -> AppResult<(UpdateStatus, Option<String>)> {
    let tag = update::latest_tag().await?;
    let status = update::status(tag.as_deref(), now_millis() / 1000);
    let fresh = state.record_update(status.clone());
    Ok((status, fresh))
}
