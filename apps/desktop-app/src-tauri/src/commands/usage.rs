//! Network usage queries.

use std::collections::HashMap;

use serde::Serialize;
use tauri::{Manager, State};

use crate::core::usage::history::{Range, Sample, SpanTimeline};
use crate::core::usage::{Ranked, UsageSummary};
use crate::error::AppResult;
use crate::state::AppState;

/// Throughput series for a range. Points are already rolled up to the tier that
/// fits the range, so the UI never receives more than ~1500 points.
#[tauri::command]
pub async fn usage_timeline(state: State<'_, AppState>, range: Range) -> AppResult<Vec<Sample>> {
    Ok(state.usage.timeline(range))
}

/// Throughput between two instants - the day, week or month the Usage page
/// is showing - at the finest tier that still covers its start. Unlike
/// [`usage_timeline`] the span is not counted back from now, so a past day can
/// be drawn too.
#[tauri::command]
pub async fn usage_timeline_between(
    state: State<'_, AppState>,
    from_ts: i64,
    to_ts: i64,
) -> AppResult<SpanTimeline> {
    Ok(state.usage.timeline_between(from_ts, to_ts))
}

/// Totals plus the ranked app / host / traffic-type columns.
#[tauri::command]
pub async fn usage_summary(state: State<'_, AppState>, range: Range) -> AppResult<UsageSummary> {
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

/// Device-wide per-app usage for a period: every app on the machine, not only
/// what passed through the tunnel.
///
/// Read from Windows' own per-app accounting, which takes seconds, so it runs
/// on a blocking worker and the UI fetches it separately from the summary
/// rather than waiting on it.
#[tauri::command]
pub async fn usage_device_apps(
    app: tauri::AppHandle,
    from_ts: i64,
    to_ts: i64,
) -> AppResult<Ranked> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        handle
            .try_state::<AppState>()
            .map(|state| state.usage.device_apps(from_ts, to_ts))
            .unwrap_or_default()
    })
    .await
    .map_err(|err| {
        log::warn!("device usage query panicked: {err}");
        crate::error::AppError::Config("could not read per-app usage".to_string())
    })
}

/// Executable icons for the named apps, as PNG data URLs. Apps without a
/// known executable or icon are simply absent from the answer.
///
/// The first request for an app reads its executable through the shell, so it
/// runs on a blocking worker; repeats are served from a cache.
#[tauri::command]
pub async fn usage_icons(
    app: tauri::AppHandle,
    apps: Vec<String>,
) -> AppResult<HashMap<String, String>> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        handle
            .try_state::<AppState>()
            .map(|state| state.usage.icons(&apps))
            .unwrap_or_default()
    })
    .await
    .map_err(|err| crate::error::AppError::Internal(format!("icon worker: {err}")))
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
