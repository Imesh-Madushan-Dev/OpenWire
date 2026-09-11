//! Wire types shared with the frontend. Every struct is `camelCase` on the wire
//! so TypeScript can mirror it without a mapping layer.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Servers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    #[default]
    Free,
    Premium,
}

/// How the client reaches the open internet once traffic enters the tunnel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Outbound {
    /// Diagnostic path: the local proxy dials the destination itself. Traffic is
    /// NOT masked - used to verify the inbound proxy and system routing work.
    Direct,

    /// VLESS over TLS with a camouflaged SNI.
    Vless(VlessConfig),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VlessConfig {
    /// Session identity. Never logged, never returned to the UI.
    #[serde(default)]
    pub uuid: String,
    /// Hostname placed in the TLS ClientHello. This is the camouflage: DPI sees
    /// a connection to this name, not to the VPS.
    #[serde(default)]
    pub sni: String,
    /// ALPN offered during the handshake; matching a real CDN improves blending.
    #[serde(default = "default_alpn")]
    pub alpn: Vec<String>,
    /// Skip certificate-name validation. Off by default: leaving it off means the
    /// masked endpoint must actually present a certificate valid for `sni`.
    #[serde(default)]
    pub allow_insecure: bool,
}

fn default_alpn() -> Vec<String> {
    vec!["h2".to_string(), "http/1.1".to_string()]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerNode {
    pub id: String,
    pub name: String,
    pub country: String,
    /// ISO-3166 alpha-2, lowercase. Drives the flag in the UI.
    pub country_code: String,
    #[serde(default)]
    pub city: String,
    /// Real TCP endpoint. Deliberately distinct from `VlessConfig::sni`.
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub tier: Tier,
    pub outbound: Outbound,
    /// 0-100 advisory load, shown as a bar in the picker.
    #[serde(default)]
    pub load: u8,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl ServerNode {
    /// A node is usable only when its outbound is fully configured.
    pub fn is_configured(&self) -> bool {
        match &self.outbound {
            Outbound::Direct => true,
            Outbound::Vless(v) => !v.uuid.is_empty() && !self.host.is_empty(),
        }
    }
}

/// Catalog entry as sent to the UI: credentials stripped, readiness flagged.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSummary {
    pub id: String,
    pub name: String,
    pub country: String,
    pub country_code: String,
    pub city: String,
    pub host: String,
    pub tier: Tier,
    pub load: u8,
    pub tags: Vec<String>,
    pub protocol: &'static str,
    pub configured: bool,
    /// Last measured TCP handshake latency, if probed.
    pub latency_ms: Option<u32>,
}

impl ServerSummary {
    pub fn from_node(node: &ServerNode, latency_ms: Option<u32>) -> Self {
        Self {
            id: node.id.clone(),
            name: node.name.clone(),
            country: node.country.clone(),
            country_code: node.country_code.clone(),
            city: node.city.clone(),
            host: node.host.clone(),
            tier: node.tier,
            load: node.load,
            tags: node.tags.clone(),
            protocol: match node.outbound {
                Outbound::Direct => "direct",
                Outbound::Vless(_) => "vless-tls",
            },
            configured: node.is_configured(),
            latency_ms,
        }
    }
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Disconnecting,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionState {
    pub phase: Phase,
    pub server_id: Option<String>,
    pub server_name: Option<String>,
    pub country_code: Option<String>,
    /// Endpoint the user's traffic exits from, shown under the status headline.
    pub exit_host: Option<String>,
    /// Loopback address apps should point at. `None` when down.
    pub listen_addr: Option<String>,
    /// Unix millis of the moment the tunnel came up.
    pub connected_at: Option<u64>,
    /// Whether the OS proxy is currently pointed at us.
    pub system_proxy: bool,
    /// Short, user-safe note ("Reconnecting", "Kill switch engaged").
    pub note: Option<String>,
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self {
            phase: Phase::Disconnected,
            server_id: None,
            server_name: None,
            country_code: None,
            exit_host: None,
            listen_addr: None,
            connected_at: None,
            system_proxy: false,
            note: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsSnapshot {
    pub up_bytes: u64,
    pub down_bytes: u64,
    pub up_bps: u64,
    pub down_bps: u64,
    pub active_connections: u64,
    pub total_connections: u64,
    pub latency_ms: Option<u32>,
    pub uptime_secs: u64,
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Port the local SOCKS5/HTTP proxy binds on 127.0.0.1.
    pub listen_port: u16,
    /// Point the OS at the local proxy while connected.
    pub system_proxy: bool,
    /// Block traffic instead of falling back to direct when the tunnel drops.
    pub kill_switch: bool,
    /// Connect to `preferred_server` on launch.
    pub auto_connect: bool,
    /// Start hidden in the tray.
    pub launch_minimized: bool,
    /// Keep LAN + loopback destinations off the tunnel.
    pub lan_bypass: bool,
    pub preferred_server: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            listen_port: 10808,
            system_proxy: true,
            kill_switch: false,
            auto_connect: false,
            launch_minimized: false,
            lan_bypass: true,
            preferred_server: None,
        }
    }
}

/// Partial update from the UI; `None` fields are left untouched.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub listen_port: Option<u16>,
    pub system_proxy: Option<bool>,
    pub kill_switch: Option<bool>,
    pub auto_connect: Option<bool>,
    pub launch_minimized: Option<bool>,
    pub lan_bypass: Option<bool>,
    /// Double option: distinguishes "absent" from "explicit null", so the
    /// preferred server can actually be cleared.
    #[serde(default, deserialize_with = "double_option")]
    pub preferred_server: Option<Option<String>>,
}

fn double_option<'de, D>(de: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(de).map(Some)
}

impl Settings {
    pub fn apply(&mut self, patch: SettingsPatch) {
        if let Some(v) = patch.listen_port {
            self.listen_port = v;
        }
        if let Some(v) = patch.system_proxy {
            self.system_proxy = v;
        }
        if let Some(v) = patch.kill_switch {
            self.kill_switch = v;
        }
        if let Some(v) = patch.auto_connect {
            self.auto_connect = v;
        }
        if let Some(v) = patch.launch_minimized {
            self.launch_minimized = v;
        }
        if let Some(v) = patch.lan_bypass {
            self.lan_bypass = v;
        }
        if let Some(v) = patch.preferred_server {
            self.preferred_server = v;
        }
    }

    /// Guards against a nonsense port arriving from the UI or a hand-edited file.
    pub fn validate(&self) -> Result<(), String> {
        if self.listen_port < 1024 {
            return Err("Choose a port between 1024 and 65535.".to_string());
        }
        Ok(())
    }
}
