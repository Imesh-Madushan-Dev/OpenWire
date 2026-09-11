//! Single owner of everything mutable in the app.
//!
//! Locks here are `std::sync` on purpose: every critical section is a field read
//! or swap with no `.await` inside, which keeps the tray's synchronous callbacks
//! able to read state without an async context.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager};

use crate::core::catalog;
use crate::core::metrics::Metrics;
use crate::core::models::{
    ConnectionState, ServerNode, ServerSummary, Settings, StatsSnapshot,
};
use crate::core::store;
use crate::core::sysproxy::{self, ProxyGuard, Recovery};
use crate::core::tunnel::Tunnel;
use crate::core::usage::UsageTracker;
use crate::error::{AppError, AppResult};

pub const EVENT_STATE: &str = "vpn:state";
pub const EVENT_STATS: &str = "vpn:stats";
pub const EVENT_NOTICE: &str = "vpn:notice";

pub struct AppState {
    app: AppHandle,
    config_dir: PathBuf,
    settings: RwLock<Settings>,
    catalog: RwLock<Vec<ServerNode>>,
    latency: RwLock<HashMap<String, u32>>,
    connection: RwLock<ConnectionState>,
    pub metrics: Arc<Metrics>,
    pub usage: Arc<UsageTracker>,
    /// Dropping either of these tears down the matching resource, so the order
    /// in `teardown` is what guarantees the OS proxy is always restored.
    tunnel: Mutex<Option<Tunnel>>,
    proxy: Mutex<Option<ProxyGuard>>,
}

impl AppState {
    pub fn load(app: &AppHandle) -> AppResult<Self> {
        let config_dir = app
            .path()
            .app_config_dir()
            .map_err(|e| AppError::Config(format!("resolve config dir: {e}")))?;

        let settings_path = config_dir.join("settings.json");
        let catalog_path = config_dir.join("servers.json");
        let usage_path = config_dir.join("usage.json");

        // Before anything else: if a previous run was killed while the system
        // proxy was engaged, the machine currently has no working networking.
        // Undoing that is more urgent than any of our own state.
        match sysproxy::recover(&config_dir.join("proxy-backup.json")) {
            Ok(Recovery::NothingToDo) => {}
            Ok(outcome) => log::warn!("recovered system proxy after an unclean exit: {outcome:?}"),
            Err(err) => log::error!("could not check for a stranded system proxy: {err}"),
        }

        let mut settings: Settings = store::read_json(&settings_path)?.unwrap_or_default();
        if settings.validate().is_err() {
            log::warn!("stored settings failed validation; reverting to defaults");
            settings = Settings::default();
        }

        // First launch seeds the user's own catalog from the bundled defaults;
        // from then on the file on disk is the source of truth.
        let nodes: Vec<ServerNode> = match store::read_json(&catalog_path)? {
            Some(nodes) => nodes,
            None => {
                let defaults = catalog::default_nodes();
                store::write_json(&catalog_path, &defaults)?;
                defaults
            }
        };

        Ok(Self {
            app: app.clone(),
            config_dir,
            settings: RwLock::new(settings),
            catalog: RwLock::new(nodes),
            latency: RwLock::new(HashMap::new()),
            connection: RwLock::new(ConnectionState::default()),
            metrics: Arc::new(Metrics::new()),
            usage: Arc::new(UsageTracker::load(&usage_path)),
            tunnel: Mutex::new(None),
            proxy: Mutex::new(None),
        })
    }

    // -- paths --------------------------------------------------------------

    fn settings_path(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    fn catalog_path(&self) -> PathBuf {
        self.config_dir.join("servers.json")
    }

    pub fn proxy_backup_path(&self) -> PathBuf {
        self.config_dir.join("proxy-backup.json")
    }

    /// Flushes usage history to disk. Cheap enough to call on a timer, and
    /// called again on exit so a clean quit never loses the last minute.
    pub fn persist_usage(&self) {
        if let Err(err) = self.usage.persist(&self.config_dir.join("usage.json")) {
            log::warn!("could not save usage history: {err}");
        }
    }

    // -- settings -----------------------------------------------------------

    pub fn settings(&self) -> Settings {
        self.settings.read().expect("settings lock").clone()
    }

    pub fn write_settings(&self, next: Settings) -> AppResult<Settings> {
        next.validate().map_err(AppError::Invalid)?;
        store::write_json(&self.settings_path(), &next)?;
        *self.settings.write().expect("settings lock") = next.clone();
        Ok(next)
    }

    // -- catalog ------------------------------------------------------------

    pub fn nodes(&self) -> Vec<ServerNode> {
        self.catalog.read().expect("catalog lock").clone()
    }

    pub fn node(&self, id: &str) -> AppResult<ServerNode> {
        self.catalog
            .read()
            .expect("catalog lock")
            .iter()
            .find(|n| n.id == id)
            .cloned()
            .ok_or_else(|| AppError::ServerNotFound(id.to_string()))
    }

    /// Credential-free view for the UI.
    pub fn summaries(&self) -> Vec<ServerSummary> {
        let latency = self.latency.read().expect("latency lock");
        self.catalog
            .read()
            .expect("catalog lock")
            .iter()
            .map(|node| ServerSummary::from_node(node, latency.get(&node.id).copied()))
            .collect()
    }

    /// Inserts or replaces a node, keyed by id, then persists.
    pub fn upsert_node(&self, node: ServerNode) -> AppResult<()> {
        {
            let mut nodes = self.catalog.write().expect("catalog lock");
            match nodes.iter_mut().find(|n| n.id == node.id) {
                Some(existing) => *existing = node,
                None => nodes.push(node),
            }
        }
        self.persist_catalog()
    }

    pub fn remove_node(&self, id: &str) -> AppResult<()> {
        {
            let mut nodes = self.catalog.write().expect("catalog lock");
            let before = nodes.len();
            nodes.retain(|n| n.id != id);
            if nodes.len() == before {
                return Err(AppError::ServerNotFound(id.to_string()));
            }
        }
        self.persist_catalog()
    }

    fn persist_catalog(&self) -> AppResult<()> {
        let nodes = self.catalog.read().expect("catalog lock").clone();
        store::write_json(&self.catalog_path(), &nodes)
    }

    pub fn record_latency(&self, results: &[(String, Option<u32>)]) {
        let mut latency = self.latency.write().expect("latency lock");
        for (id, value) in results {
            match value {
                Some(ms) => {
                    latency.insert(id.clone(), *ms);
                }
                None => {
                    latency.remove(id);
                }
            }
        }
    }

    pub fn latency_of(&self, id: &str) -> Option<u32> {
        self.latency.read().expect("latency lock").get(id).copied()
    }

    /// Best node for "Optimal location", using measured latency when available.
    pub fn optimal_node(&self) -> Option<ServerNode> {
        let nodes = self.nodes();
        catalog::optimal(&nodes, |id| self.latency_of(id)).cloned()
    }

    // -- connection ---------------------------------------------------------

    pub fn connection(&self) -> ConnectionState {
        self.connection.read().expect("connection lock").clone()
    }

    /// Mutates connection state and pushes it to the UI in one step, so the
    /// webview can never observe a state the backend has already moved past.
    pub fn update_connection(&self, mutate: impl FnOnce(&mut ConnectionState)) -> ConnectionState {
        let next = {
            let mut conn = self.connection.write().expect("connection lock");
            mutate(&mut conn);
            conn.clone()
        };
        let _ = self.app.emit(EVENT_STATE, &next);
        crate::tray::sync(&self.app, next.phase, next.server_name.as_deref());
        next
    }

    pub fn emit_stats(&self, stats: &StatsSnapshot) {
        let _ = self.app.emit(EVENT_STATS, stats);
    }

    /// User-facing, already-friendly text. Never pass raw error strings here.
    pub fn notify(&self, level: &str, message: impl Into<String>) {
        let _ = self.app.emit(
            EVENT_NOTICE,
            serde_json::json!({ "level": level, "message": message.into() }),
        );
    }

    // -- tunnel + proxy ownership -------------------------------------------

    pub fn install(&self, tunnel: Tunnel, proxy: Option<ProxyGuard>) {
        *self.tunnel.lock().expect("tunnel lock") = Some(tunnel);
        *self.proxy.lock().expect("proxy lock") = proxy;
    }

    pub fn is_tunnel_up(&self) -> bool {
        self.tunnel.lock().expect("tunnel lock").is_some()
    }

    /// Tears down in the safe order: restore the OS proxy first, then stop the
    /// listener. Reversing it would leave a window where the system points at a
    /// dead port.
    pub fn teardown(&self) {
        let proxy = self.proxy.lock().expect("proxy lock").take();
        if let Some(guard) = proxy {
            if let Err(err) = guard.release() {
                log::error!("system proxy restore failed: {err}");
                self.notify(
                    "error",
                    "Couldn't restore your system network settings. Check Windows proxy settings.",
                );
            }
        }

        if let Some(tunnel) = self.tunnel.lock().expect("tunnel lock").take() {
            tunnel.stop();
        }

        self.metrics.reset();
        self.persist_usage();
    }
}

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}
