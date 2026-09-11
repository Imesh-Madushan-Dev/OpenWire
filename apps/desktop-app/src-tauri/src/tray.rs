//! System tray: keeps the app usable while the window is closed.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::core::models::Phase;
use crate::state::AppState;

/// Kept in managed state so the connection phase can retitle the menu.
pub struct TrayHandles {
    toggle: MenuItem<Wry>,
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "Connect", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "Open Maizy", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(app, &[&toggle, &separator, &show, &quit])?;

    TrayIconBuilder::with_id("main")
        .icon(
            app.default_window_icon()
                .cloned()
                .ok_or_else(|| tauri::Error::AssetNotFound("tray icon".into()))?,
        )
        .tooltip("Maizy VPN - disconnected")
        .menu(&menu)
        // Left click opens the window; the menu stays on right click.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "toggle" => toggle_connection(app.clone()),
            "show" => focus_window(app),
            "quit" => {
                // Graceful path: `RunEvent::Exit` restores the system proxy.
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                focus_window(tray.app_handle());
            }
        })
        .build(app)?;

    app.manage(TrayHandles { toggle });
    Ok(())
}

/// Reflects the current phase in the tray menu and tooltip.
pub fn sync(app: &AppHandle, phase: Phase, server: Option<&str>) {
    let Some(handles) = app.try_state::<TrayHandles>() else {
        return;
    };

    let label = match phase {
        Phase::Disconnected => "Connect",
        Phase::Connecting => "Connecting...",
        Phase::Connected | Phase::Reconnecting => "Disconnect",
        Phase::Disconnecting => "Disconnecting...",
    };
    let _ = handles.toggle.set_text(label);
    // Mid-transition clicks would race the state machine.
    let _ = handles
        .toggle
        .set_enabled(!matches!(phase, Phase::Connecting | Phase::Disconnecting));

    let tooltip = match (phase, server) {
        (Phase::Connected, Some(name)) => format!("Maizy VPN - connected to {name}"),
        (Phase::Connected, None) => "Maizy VPN - connected".to_string(),
        (Phase::Reconnecting, _) => "Maizy VPN - reconnecting".to_string(),
        (Phase::Connecting, _) => "Maizy VPN - connecting".to_string(),
        (Phase::Disconnecting, _) => "Maizy VPN - disconnecting".to_string(),
        (Phase::Disconnected, _) => "Maizy VPN - disconnected".to_string(),
    };
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(tooltip));
    }
}

pub fn focus_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn toggle_connection(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Err(err) = crate::commands::vpn::vpn_toggle(app.clone(), state, None).await {
            log::warn!("tray toggle failed: {err}");
            let state = app.state::<AppState>();
            state.notify("error", err.user_message());
        }
    });
}
