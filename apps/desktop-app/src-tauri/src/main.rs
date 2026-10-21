// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // The same binary is also the network monitor service and its elevated
    // installer; those roles exit here, before Tauri starts.
    if let Some(code) = app_lib::netmon_dispatch() {
        std::process::exit(code);
    }
    app_lib::run();
}
