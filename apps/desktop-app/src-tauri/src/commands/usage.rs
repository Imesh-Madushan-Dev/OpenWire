//! Network usage queries.

use serde::Serialize;
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

/// Totals plus columns for an arbitrary period - a named calendar month, or a
/// custom start and end the user picked.
///
/// Both bounds are timestamps anywhere inside the wanted first and last day.
/// Sending instants rather than date strings means the backend keeps its single
/// definition of a local day, instead of the UI reimplementing the host's
/// timezone and the two disagreeing across a DST boundary.
#[tauri::command]
pub async fn usage_summary_between(
    state: State<'_, AppState>,
    from_ts: i64,
    to_ts: i64,
) -> AppResult<UsageSummary> {
    Ok(state.usage.summary_between(from_ts, to_ts))
}

/// The period the store actually holds, so the UI can offer months that have
/// data and bound the custom-range pickers, rather than letting the user select
/// a window that can only come back empty.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSpan {
    /// Midday of the first and last recorded local day. `None` when nothing
    /// has been recorded yet.
    pub first_ts: Option<i64>,
    pub last_ts: Option<i64>,
    /// How many days of history are kept at most.
    pub retention_days: u32,
}

#[tauri::command]
pub async fn usage_span(state: State<'_, AppState>) -> AppResult<UsageSpan> {
    let span = state.usage.recorded_span();
    Ok(UsageSpan {
        first_ts: span.map(|(first, _)| first),
        last_ts: span.map(|(_, last)| last),
        retention_days: crate::core::usage::history::DAYS_KEPT as u32,
    })
}
