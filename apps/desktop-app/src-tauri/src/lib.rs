//! Maizy VPN - Tauri application entry point.

mod commands;
mod core;
mod error;
mod state;
mod tray;

use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, RunEvent, WindowEvent};

use crate::core::metrics::RateSampler;
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Must happen before any TLS config is built, and exactly once per process.
    if rustls::crypto::ring::default_provider()
        .install_default()
        .is_err()
    {
        log::debug!("rustls provider was already installed");
    }

    let app = tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::vpn::vpn_status,
            commands::vpn::vpn_stats,
            commands::vpn::vpn_connect,
            commands::vpn::vpn_disconnect,
            commands::vpn::vpn_toggle,
            commands::servers::servers_list,
            commands::servers::servers_probe,
            commands::servers::servers_import,
            commands::servers::servers_remove,
            commands::servers::servers_optimal,
            commands::settings::settings_get,
            commands::settings::settings_update,
            commands::settings::platform_info,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            let state = AppState::load(&handle)?;
            let settings = state.settings();
            app.manage(state);

            tray::build(&handle)?;
            spawn_stats_ticker(handle.clone());
            spawn_initial_probe(handle.clone());

            if settings.launch_minimized {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            if settings.auto_connect {
                spawn_auto_connect(handle);
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window parks the app in the tray; the tunnel keeps
            // running, which is the whole point of a tray VPN client.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to start Maizy VPN");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            // Last chance to hand the user's network settings back.
            if let Some(state) = handle.try_state::<AppState>() {
                state.teardown();
            }
        }
    });
}

/// Pushes a throughput sample to the UI once a second while connected.
fn spawn_stats_ticker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut sampler = RateSampler::default();
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        let mut last_tick = Instant::now();

        loop {
            ticker.tick().await;

            let Some(state) = app.try_state::<AppState>() else {
                break;
            };

            if !state.is_tunnel_up() {
                // Avoid a false spike on the first tick of the next session.
                sampler.reset();
                last_tick = Instant::now();
                continue;
            }

            let elapsed = last_tick.elapsed().as_millis() as u64;
            last_tick = Instant::now();

            let mut snapshot = commands::vpn::snapshot_with_uptime(&state);
            sampler.sample(&mut snapshot, elapsed);
            state.emit_stats(&snapshot);
        }
    });
}

/// Warms the latency table so "Optimal location" means something on first open.
fn spawn_initial_probe(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let targets: Vec<(String, String, u16)> = {
            let Some(state) = app.try_state::<AppState>() else {
                return;
            };
            state
                .nodes()
                .into_iter()
                .filter(|n| n.is_configured() && !n.host.is_empty() && n.port != 0)
                .map(|n| (n.id, n.host, n.port))
                .collect()
        };

        if targets.is_empty() {
            return;
        }

        let results = core::probe::probe_all(targets).await;
        if let Some(state) = app.try_state::<AppState>() {
            state.record_latency(&results);
        }
    });
}

fn spawn_auto_connect(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Let the initial probe land so "optimal" picks a measured node.
        tokio::time::sleep(Duration::from_secs(2)).await;

        let state = app.state::<AppState>();
        if let Err(err) = commands::vpn::vpn_connect(app.clone(), state, None).await {
            log::warn!("auto-connect failed: {err}");
            let state = app.state::<AppState>();
            state.notify("error", err.user_message());
        }
    });
}
