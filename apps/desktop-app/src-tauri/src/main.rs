// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // The same binary is also the network monitor and tunnel services and
    // their elevated installers; those roles exit here, before Tauri starts.
    if let Some(code) = app_lib::service_dispatch() {
        std::process::exit(code);
    }
    app_lib::run();
}
