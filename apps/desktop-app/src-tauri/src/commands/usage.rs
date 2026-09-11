//! Network usage queries.

use tauri::State;

use crate::core::usage::history::{Range, Sample};
use crate::core::usage::UsageSummary;
use crate::error::AppResult;
use crate::state::AppState;

/// Throughput series for a range. Points are already rolled up to the tier that
/// fits the range, so the UI never receives more than ~1500 points.
#[tauri::command]
pub async fn usage_timeline(
    state: State<'_, AppState>,
    range: Range,
) -> AppResult<Vec<Sample>> {
    Ok(state.usage.timeline(range))
}

/// Totals plus the ranked app / host / traffic-type columns.
#[tauri::command]
pub async fn usage_summary(
    state: State<'_, AppState>,
    range: Range,
) -> AppResult<UsageSummary> {
    Ok(state.usage.summary(range))
}
