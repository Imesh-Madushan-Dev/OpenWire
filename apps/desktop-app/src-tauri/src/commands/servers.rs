//! Server catalog commands. Credentials never cross this boundary - the UI only
//! ever receives [`ServerSummary`] - with one deliberate exception:
//! [`servers_share_link`], which exists to hand the user their own link.

use serde::Serialize;
use tauri::State;

use crate::core::models::{ServerEdit, ServerSummary};
use crate::core::{catalog, clipboard, probe};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[tauri::command]
pub async fn servers_list(state: State<'_, AppState>) -> AppResult<Vec<ServerSummary>> {
    Ok(state.summaries())
}

/// Measures every reachable node so the picker can sort by real latency.
#[tauri::command]
pub async fn servers_probe(state: State<'_, AppState>) -> AppResult<Vec<ServerSummary>> {
    let targets: Vec<(String, String, u16)> = state
        .nodes()
        .into_iter()
        .filter(|n| n.is_configured() && !n.host.is_empty() && n.port != 0)
        .map(|n| (n.id, n.host, n.port))
        .collect();

    let results = probe::probe_all(targets).await;
    state.record_latency(&results);
    Ok(state.summaries())
}

/// Imports one share link (`vless://` or `trojan://`). This is how a user adds
/// their own VPS.
#[tauri::command]
pub async fn servers_import(state: State<'_, AppState>, uri: String) -> AppResult<ServerSummary> {
    let node = catalog::parse_share_link(&uri)?;
    state.upsert_node(node.clone())?;
    log::info!("imported server {} ({})", node.name, node.id);
    Ok(ServerSummary::from_node(&node, None))
}

/// The outcome of importing many links at once.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: Vec<ServerSummary>,
    /// Links found but not imported - unsupported protocols, mostly.
    pub failed: usize,
    /// Why the first failure failed, in words for the user.
    pub first_error: Option<String>,
}

/// Imports every link in a block of text: a pasted list, a file's contents,
/// a decoded QR code, or a base64 subscription body.
#[tauri::command]
pub async fn servers_import_text(
    state: State<'_, AppState>,
    text: String,
) -> AppResult<ImportReport> {
    import_all(&state, &text)
}

/// As [`servers_import_text`], reading the clipboard here so the link never
/// passes through the webview.
#[tauri::command]
pub async fn servers_import_clipboard(state: State<'_, AppState>) -> AppResult<ImportReport> {
    let text = tauri::async_runtime::spawn_blocking(clipboard::read_text)
        .await
        .ok()
        .flatten()
        .ok_or_else(|| AppError::Invalid("Your clipboard has no text in it.".to_string()))?;
    import_all(&state, &text)
}

fn import_all(state: &AppState, text: &str) -> AppResult<ImportReport> {
    let links = catalog::extract_links(text);
    if links.is_empty() {
        return Err(AppError::Invalid(
            "No server links found. They start with vless:// or trojan://.".to_string(),
        ));
    }

    let mut report = ImportReport {
        imported: Vec::new(),
        failed: 0,
        first_error: None,
    };
    for link in links {
        match catalog::parse_share_link(&link) {
            Ok(node) => {
                // A list may repeat a server; the last copy wins, once.
                report.imported.retain(|summary| summary.id != node.id);
                report.imported.push(ServerSummary::from_node(&node, None));
                state.upsert_node(node)?;
            }
            Err(err) => {
                report.failed += 1;
                report.first_error.get_or_insert_with(|| err.user_message());
            }
        }
    }

    log::info!(
        "imported {} server(s), {} link(s) skipped",
        report.imported.len(),
        report.failed
    );
    Ok(report)
}

/// The server's share link, credentials included, for the user to copy or
/// show as a QR code. Only ever called from an explicit Share action.
#[tauri::command]
pub async fn servers_share_link(state: State<'_, AppState>, id: String) -> AppResult<String> {
    let node = state.node(&id)?;
    catalog::share_link(&node)
        .ok_or_else(|| AppError::Invalid("This route has no link to share.".to_string()))
}

/// Renames a server or changes how it is reached. Credentials are not
/// editable - a new key means importing the server's new link.
#[tauri::command]
pub async fn servers_update(
    state: State<'_, AppState>,
    id: String,
    edit: ServerEdit,
) -> AppResult<ServerSummary> {
    // A live session keeps the settings it started with; changing them under
    // it would leave the UI describing a connection that is not the real one.
    let in_use =
        state.connection().server_id.as_deref() == Some(id.as_str()) && state.is_tunnel_up();
    if in_use && edit.touches_connection() {
        return Err(AppError::Invalid(
            "Disconnect before changing how you reach the server you're using.".to_string(),
        ));
    }

    let mut node = state.node(&id)?;
    catalog::apply_edit(&mut node, edit)?;
    state.upsert_node(node.clone())?;
    Ok(ServerSummary::from_node(&node, state.latency_of(&node.id)))
}

#[tauri::command]
pub async fn servers_remove(state: State<'_, AppState>, id: String) -> AppResult<()> {
    // Removing the node you are currently on would leave the session orphaned.
    if state.connection().server_id.as_deref() == Some(id.as_str()) && state.is_tunnel_up() {
        return Err(AppError::Invalid(
            "Disconnect before removing the server you're using.".to_string(),
        ));
    }

    state.remove_node(&id)?;

    // Drop a stale preference so the next auto-connect does not point at a gap.
    let mut settings = state.settings();
    if settings.preferred_server.as_deref() == Some(id.as_str()) {
        settings.preferred_server = None;
        state.write_settings(settings)?;
    }
    Ok(())
}

/// The node behind "Optimal location".
#[tauri::command]
pub async fn servers_optimal(state: State<'_, AppState>) -> AppResult<Option<ServerSummary>> {
    Ok(state.optimal_node().map(|node| {
        let latency = state.latency_of(&node.id);
        ServerSummary::from_node(&node, latency)
    }))
}
