//! Connect / disconnect / status.

use std::sync::Arc;

use tauri::{AppHandle, Manager, State};
use tokio::sync::mpsc;

use crate::core::models::{ConnectionState, Outbound, Phase, ServerNode, StatsSnapshot};
use crate::core::sysproxy::{self, ProxyGuard};
use crate::core::tunnel::{Tunnel, TunnelConfig, TunnelEvent};
use crate::error::{AppError, AppResult};
use crate::state::{now_millis, AppState};

/// The bundled node monitor mode routes through: no remote, so the local proxy
/// dials destinations itself.
const MONITOR_NODE_ID: &str = "maizy-direct";

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
    // Monitor mode is holding a tunnel of its own. A real connection replaces
    // it rather than being refused as a duplicate.
    if state.is_monitoring() {
        state.teardown();
        state.update_connection(|conn| *conn = ConnectionState::default());
    } else if state.is_tunnel_up() {
        return Err(AppError::AlreadyConnected);
    }

    let node = resolve_node(&state, server_id)?;
    bring_up(app, state.inner(), node, false).await
}

/// Brings up the local route with no remote, so traffic can still be metered
/// and attributed while no VPN is connected.
///
/// This is the same listener, the same system proxy and the same per-connection
/// accounting a VPN session uses - only the outbound hop is missing. It is what
/// lets the usage breakdown name apps and hosts without a kernel driver.
pub async fn start_monitor(app: AppHandle, state: &AppState) -> AppResult<ConnectionState> {
    if state.is_tunnel_up() {
        return Err(AppError::AlreadyConnected);
    }

    // With the OS not pointed at us nothing would arrive, so there would be
    // nothing to meter - better to skip than hold a listener for show.
    if !state.settings().system_proxy {
        return Err(AppError::Invalid(
            "Monitor mode needs the system proxy, which is switched off.".to_string(),
        ));
    }
    if !sysproxy::is_supported() {
        return Err(AppError::Invalid(
            "Monitor mode needs automatic system routing, which this platform does not have."
                .to_string(),
        ));
    }

    // The node is removable from the user's catalog, and monitor mode does not
    // need their copy of it - it carries no credentials and no remote. Falling
    // back to the bundled definition keeps the feature working after a removal.
    let node = state.node(MONITOR_NODE_ID).or_else(|_| {
        monitor_node()
            .ok_or_else(|| AppError::Invalid("Monitor mode has no local route to use.".to_string()))
    })?;

    bring_up(app, state, node, true).await
}

fn monitor_node() -> Option<ServerNode> {
    crate::core::catalog::direct_node()
}

/// Starts the listener for `node` and points the OS at it.
///
/// `monitoring` changes only how the result is labelled and whether the UI is
/// walked through a "Connecting" phase; the routing is byte-for-byte identical.
async fn bring_up(
    app: AppHandle,
    state: &AppState,
    node: ServerNode,
    monitoring: bool,
) -> AppResult<ConnectionState> {
    let settings = state.settings();

    // Monitor mode starts itself on launch and after a disconnect. Flashing
    // "Connecting" there would read as a VPN coming up, which it is not.
    if !monitoring {
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
    }

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
        conn.phase = if monitoring {
            Phase::Monitoring
        } else {
            Phase::Connected
        };
        // Set here rather than only in the "Connecting" hop above, which
        // monitor mode skips.
        conn.server_id = Some(node.id.clone());
        conn.server_name = Some(node.name.clone());
        conn.country_code = Some(node.country_code.clone());
        conn.exit_host = Some(exit_label(&node));
        conn.listen_addr = Some(listen_addr.to_string());
        conn.connected_at = Some(now_millis());
        conn.system_proxy = proxy_engaged;
        conn.note = if monitoring {
            Some("Measuring usage only - your traffic is not masked.".to_string())
        } else {
            match node.outbound {
                // Be explicit that the diagnostic node does not mask anything.
                Outbound::Direct => Some("Diagnostic route - traffic is not masked.".to_string()),
                Outbound::Vless(_) | Outbound::Trojan(_) => None,
            }
        };
    }))
}

#[tauri::command]
pub async fn vpn_disconnect(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<ConnectionState> {
    if !state.is_tunnel_up() {
        return Err(AppError::NotConnected);
    }

    state.update_connection(|conn| {
        conn.phase = Phase::Disconnecting;
        conn.note = None;
    });

    state.teardown();
    state.update_connection(|conn| *conn = ConnectionState::default());

    // Dropping the VPN must not also stop metering: monitor mode takes the
    // route back, so the usage breakdown keeps naming apps and hosts.
    if state.settings().monitor_when_idle {
        match start_monitor(app, state.inner()).await {
            Ok(conn) => return Ok(conn),
            Err(err) => log::info!("monitor mode not resumed: {err}"),
        }
    }

    Ok(state.connection())
}

/// Single entry point for the tray and the UI toggle.
#[tauri::command]
pub async fn vpn_toggle(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: Option<String>,
) -> AppResult<ConnectionState> {
    // Monitor mode holds a tunnel but is not a connection, so the toggle must
    // connect from it rather than tear it down.
    if state.is_tunnel_up() && !state.is_monitoring() {
        vpn_disconnect(app, state).await
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
            "No server is set up yet. Add one from its share link on the VPN page.".to_string(),
        )
    })
}

fn exit_label(node: &ServerNode) -> String {
    match node.outbound {
        Outbound::Direct => "This device".to_string(),
        Outbound::Vless(_) | Outbound::Trojan(_) => node.host.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Monitor mode resolves its route by id, so renaming or dropping the
    /// bundled direct node would make it silently never start.
    #[test]
    fn the_monitor_node_exists_and_needs_no_credentials() {
        let node = monitor_node().expect("a bundled node must back monitor mode");

        assert!(
            matches!(node.outbound, Outbound::Direct),
            "monitor mode must not depend on a remote server"
        );
        assert!(
            node.is_configured(),
            "monitor mode must work before any server is set up"
        );
    }
}
