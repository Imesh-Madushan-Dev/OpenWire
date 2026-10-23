//! OpenWire - Tauri application entry point.

mod commands;
mod core;
mod error;
mod state;
mod taskbar;
mod tray;

use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};

use crate::core::metrics::RateSampler;
use crate::state::AppState;

/// Live throughput tick for the usage graph.
const EVENT_USAGE_SAMPLE: &str = "usage:sample";
/// The network monitor service came, went, or changed state.
const EVENT_NETMON_STATUS: &str = "netmon:status";

/// Runs the network monitor's roles - the service and its elevated
/// installer - when the command line asks for one. They share this binary
/// but must never start Tauri, so `main` checks this first.
pub fn netmon_dispatch() -> Option<i32> {
    core::netmon::dispatch()
}

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
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
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
            commands::vpn::vpn_logs,
            commands::vpn::vpn_logs_clear,
            commands::servers::servers_list,
            commands::servers::servers_probe,
            commands::servers::servers_import,
            commands::servers::servers_import_text,
            commands::servers::servers_import_clipboard,
            commands::servers::servers_share_link,
            commands::servers::servers_update,
            commands::servers::servers_remove,
            commands::servers::servers_optimal,
            commands::settings::settings_get,
            commands::settings::settings_update,
            commands::settings::platform_info,
            commands::settings::network_restore,
            commands::settings::app_icon_set,
            commands::update::update_status,
            commands::update::update_check,
            commands::usage::usage_timeline,
            commands::usage::usage_timeline_between,
            commands::usage::usage_summary,
            commands::usage::usage_summary_between,
            commands::usage::usage_span,
            commands::usage::usage_device_apps,
            commands::usage::usage_icons,
            commands::netmon::netmon_status,
            commands::netmon::netmon_enable,
            commands::netmon::netmon_disable,
            commands::devices::devices_snapshot,
            commands::devices::devices_scan,
            commands::devices::devices_rename,
            commands::devices::devices_set_kind,
            commands::devices::devices_acknowledge,
            commands::devices::devices_forget,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            let state = AppState::load(&handle)?;
            let settings = state.settings();

            let notifier = handle.clone();
            state
                .netmon
                .spawn(std::sync::Arc::clone(&state.usage), move |status| {
                    let _ = notifier.emit(EVENT_NETMON_STATUS, &status);
                });
            app.manage(state);

            tray::build(&handle)?;
            taskbar::set_enabled(&handle, settings.taskbar_speed);
            spawn_stats_ticker(handle.clone());
            spawn_initial_probe(handle.clone());
            spawn_device_watch(handle.clone());
            spawn_update_watch(handle.clone());

            if settings.launch_minimized {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            spawn_startup_route(handle, settings.auto_connect, settings.monitor_when_idle);

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
        .expect("failed to start OpenWire");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            // Last chance to hand the user's network settings back.
            if let Some(state) = handle.try_state::<AppState>() {
                state.teardown();
            }
            // Out of Explorer's taskbar before the process goes.
            taskbar::set_enabled(handle, false);
        }
    });
}

/// One-second heartbeat: samples network usage always, and pushes VPN
/// throughput while a tunnel is up.
fn spawn_stats_ticker(app: AppHandle) {
    // Usage history is flushed on a slower cadence than it is sampled; a crash
    // costs at most this much history, and the disk stays quiet.
    const PERSIST_EVERY_TICKS: u32 = 60;

    tauri::async_runtime::spawn(async move {
        let mut sampler = RateSampler::default();
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        let mut last_tick = Instant::now();
        let mut ticks: u32 = 0;

        loop {
            ticker.tick().await;

            let Some(state) = app.try_state::<AppState>() else {
                break;
            };

            let sample = state.usage.sample();
            let _ = app.emit(EVENT_USAGE_SAMPLE, &sample);
            taskbar::record(sample.up, sample.down);

            ticks = ticks.wrapping_add(1);
            if ticks % PERSIST_EVERY_TICKS == 0 {
                state.persist_usage();
            }

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

/// Sweeps the local network in the background, so the Devices list is
/// current when opened and a newcomer is noticed - first shortly after
/// launch, once the network has settled, then every few minutes.
fn spawn_device_watch(app: AppHandle) {
    const FIRST_AFTER: Duration = Duration::from_secs(20);
    const EVERY: Duration = Duration::from_secs(5 * 60);

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_AFTER).await;
        loop {
            let Some(state) = app.try_state::<AppState>() else {
                break;
            };
            // Checked every round, so turning it off takes effect without a restart.
            if state.settings().watch_devices && !state.lan.is_scanning() {
                if let Err(err) = commands::devices::scan(&app).await {
                    log::debug!("background device sweep: {err}");
                }
            }
            tokio::time::sleep(EVERY).await;
        }
    });
}

/// Asks GitHub whether a newer OpenWire is out - shortly after launch, once
/// the network has settled, then twice a day for a tray app left running -
/// and says so once per release.
fn spawn_update_watch(app: AppHandle) {
    const FIRST_AFTER: Duration = Duration::from_secs(30);
    const EVERY: Duration = Duration::from_secs(12 * 60 * 60);
    /// A failed check is tried again sooner: networks that reset some TLS
    /// handshakes would otherwise cost half a day.
    const RETRY_AFTER: Duration = Duration::from_secs(30 * 60);

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_AFTER).await;
        loop {
            let Some(state) = app.try_state::<AppState>() else {
                break;
            };
            let mut wait = EVERY;
            // Checked every round, so turning it off takes effect without a restart.
            if state.settings().check_updates {
                match commands::update::check(&state).await {
                    Ok((_, Some(version))) => state.notify(
                        "info",
                        format!("OpenWire {version} is available - download it from Settings > About."),
                    ),
                    Ok(_) => {}
                    // Offline, or GitHub is blocked here: nothing worth interrupting over.
                    Err(err) => {
                        log::debug!("background update check: {err}");
                        wait = RETRY_AFTER;
                    }
                }
            }
            tokio::time::sleep(wait).await;
        }
    });
}

/// Warms the latency table so "Optimal location" means something on first open.
fn spawn_initial_probe(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let targets: Vec<(String, core::probe::Target)> = {
            let Some(state) = app.try_state::<AppState>() else {
                return;
            };
            state
                .nodes()
                .into_iter()
                .filter(|n| n.is_configured() && !n.host.is_empty() && n.port != 0)
                .map(|n| (n.id.clone(), core::probe::Target::of(&n)))
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

/// Auto-connects if asked, and otherwise falls back to monitor mode so the
/// usage breakdown has something to attribute from launch.
fn spawn_startup_route(app: AppHandle, auto_connect: bool, monitor_when_idle: bool) {
    if !auto_connect && !monitor_when_idle {
        return;
    }

    tauri::async_runtime::spawn(async move {
        if auto_connect {
            // Let the initial probe land so "optimal" picks a measured node.
            tokio::time::sleep(Duration::from_secs(2)).await;

            let state = app.state::<AppState>();
            match commands::vpn::vpn_connect(app.clone(), state, None).await {
                Ok(_) => return,
                Err(err) => {
                    log::warn!("auto-connect failed: {err}");
                    app.state::<AppState>().notify("error", err.user_message());
                }
            }
        }

        if monitor_when_idle {
            let state = app.state::<AppState>();
            // Not reaching the user: monitor mode is a convenience, and its
            // reasons for not starting are not something to interrupt over.
            if let Err(err) = commands::vpn::start_monitor(app.clone(), state.inner()).await {
                log::info!("monitor mode not started: {err}");
            }
        }
    });
}
