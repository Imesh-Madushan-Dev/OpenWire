//! Connect / disconnect / status.

use std::sync::Arc;

use tauri::{AppHandle, Manager, State};
use tokio::sync::mpsc;

use crate::core::models::{ConnectionState, Outbound, Phase, ServerNode, StatsSnapshot};
use crate::core::sysproxy::{self, ProxyGuard};
use crate::core::tunnel::{Tunnel, TunnelConfig, TunnelEvent};
use crate::error::{AppError, AppResult};
use crate::state::{now_millis, AppState};

#[tauri::command]
pub async fn vpn_status(state: State<'_, AppState>) -> AppResult<ConnectionState> {
    Ok(state.connection())
}

#[tauri::command]
pub async fn vpn_stats(state: State<'_, AppState>) -> AppResult<StatsSnapshot> {
    Ok(snapshot_with_uptime(&state))
}

#[tauri::command]
pub async fn vpn_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: Option<String>,
) -> AppResult<ConnectionState> {
    if state.is_tunnel_up() {
        return Err(AppError::AlreadyConnected);
    }

    let settings = state.settings();
    let node = resolve_node(&state, server_id)?;

    state.update_connection(|conn| {
        conn.phase = Phase::Connecting;
        conn.server_id = Some(node.id.clone());
        conn.server_name = Some(node.name.clone());
        conn.country_code = Some(node.country_code.clone());
        conn.exit_host = Some(exit_label(&node));
        conn.listen_addr = None;
        conn.connected_at = None;
        conn.system_proxy = false;
        conn.note = None;
    });

    // Zero before the tunnel starts: the health watcher writes latency almost
    // immediately, and resetting afterwards would wipe the first reading.
    state.metrics.reset();

    let (events_tx, events_rx) = mpsc::unbounded_channel();
    let tunnel = match Tunnel::start(
        TunnelConfig {
            listen_port: settings.listen_port,
            node: node.clone(),
            lan_bypass: settings.lan_bypass,
            kill_switch: settings.kill_switch,
        },
        Arc::clone(&state.metrics),
        Arc::clone(&state.usage),
        events_tx,
    )
    .await
    {
        Ok(tunnel) => tunnel,
        Err(err) => {
            // Roll all the way back so a failed attempt cannot leave the UI
            // stuck on "Connecting".
            let message = err.user_message();
            state.update_connection(|conn| {
                *conn = ConnectionState::default();
                conn.note = Some(message);
            });
            return Err(err);
        }
    };

    let listen_addr = tunnel.local_addr;

    // The OS proxy is best-effort: a failure here still leaves a usable tunnel
    // for apps pointed at the loopback address manually, so warn rather than
    // tear the session down.
    let mut proxy_engaged = false;
    let mut guard: Option<ProxyGuard> = None;
    if settings.system_proxy {
        if sysproxy::is_supported() {
            match ProxyGuard::engage(
                listen_addr.port(),
                settings.lan_bypass,
                &state.proxy_backup_path(),
            ) {
                Ok(engaged) => {
                    guard = Some(engaged);
                    proxy_engaged = true;
                }
                Err(err) => {
                    log::error!("system proxy engage failed: {err}");
                    state.notify("warning", err.user_message());
                }
            }
        } else {
            state.notify(
                "info",
                format!("Point your apps at {listen_addr} to use the tunnel on this platform."),
            );
        }
    }

    state.install(tunnel, guard);
    spawn_health_forwarder(app, events_rx);

    Ok(state.update_connection(|conn| {
        conn.phase = Phase::Connected;
        conn.listen_addr = Some(listen_addr.to_string());
        conn.connected_at = Some(now_millis());
        conn.system_proxy = proxy_engaged;
        conn.note = match node.outbound {
            // Be explicit that the diagnostic node does not mask anything.
            Outbound::Direct => Some("Diagnostic route - traffic is not masked.".to_string()),
            Outbound::Vless(_) => None,
        };
    }))
}

#[tauri::command]
pub async fn vpn_disconnect(state: State<'_, AppState>) -> AppResult<ConnectionState> {
    if !state.is_tunnel_up() {
        return Err(AppError::NotConnected);
    }

    state.update_connection(|conn| {
        conn.phase = Phase::Disconnecting;
        conn.note = None;
    });

    state.teardown();

    Ok(state.update_connection(|conn| *conn = ConnectionState::default()))
}

/// Single entry point for the tray and the UI toggle.
#[tauri::command]
pub async fn vpn_toggle(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: Option<String>,
) -> AppResult<ConnectionState> {
    if state.is_tunnel_up() {
        vpn_disconnect(state).await
    } else {
        vpn_connect(app, state, server_id).await
    }
}

// ---------------------------------------------------------------------------

fn resolve_node(state: &AppState, requested: Option<String>) -> AppResult<ServerNode> {
    if let Some(id) = requested {
        let node = state.node(&id)?;
        if !node.is_configured() {
            return Err(AppError::MissingCredentials(node.name));
        }
        return Ok(node);
    }

    // Fall back to the saved preference, then to whatever is fastest.
    let preferred = state
        .settings()
        .preferred_server
        .and_then(|id| state.node(&id).ok())
        .filter(ServerNode::is_configured);

    preferred.or_else(|| state.optimal_node()).ok_or_else(|| {
        AppError::Invalid(
            "No server is set up yet. Add one with a vless:// link in Settings.".to_string(),
        )
    })
}

fn exit_label(node: &ServerNode) -> String {
    match node.outbound {
        Outbound::Direct => "This device".to_string(),
        Outbound::Vless(_) => node.host.clone(),
    }
}

pub fn snapshot_with_uptime(state: &AppState) -> StatsSnapshot {
    let mut snap = state.metrics.snapshot();
    if let Some(started) = state.connection().connected_at {
        snap.uptime_secs = now_millis().saturating_sub(started) / 1000;
    }
    snap
}

/// Translates tunnel health into user-visible connection phases.
fn spawn_health_forwarder(app: AppHandle, mut rx: mpsc::UnboundedReceiver<TunnelEvent>) {
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let Some(state) = app.try_state::<AppState>() else {
                break;
            };
            // A late event from a session that has already ended must not
            // resurrect the UI into a connected state.
            if !state.is_tunnel_up() {
                continue;
            }

            match event {
                TunnelEvent::Degraded => {
                    let kill_switch = state.settings().kill_switch;
                    state.update_connection(|conn| {
                        conn.phase = Phase::Reconnecting;
                        conn.note = Some(if kill_switch {
                            "Server unreachable. Kill switch is blocking traffic.".to_string()
                        } else {
                            "Server unreachable. Retrying...".to_string()
                        });
                    });
                }
                TunnelEvent::Healthy { .. } => {
                    state.update_connection(|conn| {
                        conn.phase = Phase::Connected;
                        conn.note = None;
                    });
                }
            }
        }
    });
}
