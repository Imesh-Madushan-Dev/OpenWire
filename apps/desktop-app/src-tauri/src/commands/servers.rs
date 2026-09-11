//! Server catalog commands. Credentials never cross this boundary - the UI only
//! ever receives [`ServerSummary`].

use tauri::State;

use crate::core::models::ServerSummary;
use crate::core::{catalog, probe};
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

/// Imports a `vless://` share link. This is how a user adds their own VPS.
#[tauri::command]
pub async fn servers_import(state: State<'_, AppState>, uri: String) -> AppResult<ServerSummary> {
    let node = catalog::parse_vless_uri(&uri)?;
    state.upsert_node(node.clone())?;
    log::info!("imported server {} ({})", node.name, node.id);
    Ok(ServerSummary::from_node(&node, None))
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
    Ok(state
        .optimal_node()
        .map(|node| {
            let latency = state.latency_of(&node.id);
            ServerSummary::from_node(&node, latency)
        }))
}
