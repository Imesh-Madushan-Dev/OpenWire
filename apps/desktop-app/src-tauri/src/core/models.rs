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

    /// VLESS, usually over TLS with a camouflaged SNI.
    Vless(VlessConfig),

    /// Trojan, masked the same way.
    Trojan(TrojanConfig),
}

impl Outbound {
    /// Wire label for the UI and the logs.
    pub fn protocol(&self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Vless(_) => "vless",
            Self::Trojan(_) => "trojan",
        }
    }

    /// The TLS settings, for every outbound that speaks TLS. `None` for the
    /// direct route and for servers reached without TLS.
    pub fn tls(&self) -> Option<TlsSettings<'_>> {
        let (security, settings) = match self {
            Self::Direct => return None,
            Self::Vless(config) => (
                config.security,
                TlsSettings {
                    sni: &config.sni,
                    alpn: &config.alpn,
                    allow_insecure: config.allow_insecure,
                },
            ),
            Self::Trojan(config) => (
                config.security,
                TlsSettings {
                    sni: &config.sni,
                    alpn: &config.alpn,
                    allow_insecure: config.allow_insecure,
                },
            ),
        };
        (security == Security::Tls).then_some(settings)
    }

    /// How the protocol's bytes are framed on the way to the server.
    pub fn transport(&self) -> Option<&Transport> {
        match self {
            Self::Direct => None,
            Self::Vless(config) => Some(&config.transport),
            Self::Trojan(config) => Some(&config.transport),
        }
    }
}

/// Whether the connection to the server is wrapped in TLS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Security {
    #[default]
    Tls,
    /// `security=none`: no TLS at all, usually a port-80 server behind a CDN.
    /// Nothing but the transport's framing stands between the tunnel and an
    /// observer, so there is no SNI to mask with either.
    None,
}

/// How a protocol's bytes travel once the connection to the server is open:
/// straight down it, or framed inside HTTP so the server can sit behind a CDN
/// or a web server. Named after a share link's `type=`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Transport {
    /// The protocol written straight to the connection.
    #[default]
    Tcp,

    /// Raw TCP behind one fake HTTP/1.1 request and response head
    /// (`type=tcp&headerType=http`). Both fields are comma-separated lists;
    /// each connection picks one of each.
    #[serde(rename = "tcp-http")]
    TcpHttp {
        #[serde(default)]
        host: String,
        #[serde(default)]
        path: String,
    },

    /// WebSocket. Up to `early_data` bytes of the protocol head ride in the
    /// upgrade request itself, saving the server a round trip.
    #[serde(rename_all = "camelCase")]
    Ws {
        #[serde(default)]
        path: String,
        #[serde(default)]
        host: String,
        #[serde(default)]
        early_data: usize,
    },

    /// An HTTP/1.1 upgrade request, then the raw stream with no framing.
    HttpUpgrade {
        #[serde(default)]
        path: String,
        #[serde(default)]
        host: String,
    },

    /// gRPC "gun": one bidirectional stream, each chunk a protobuf message.
    #[serde(rename_all = "camelCase")]
    Grpc {
        #[serde(default)]
        service_name: String,
        #[serde(default)]
        authority: String,
        /// `TunMulti` instead of `Tun`, as `mode=multi` asks.
        #[serde(default)]
        multi: bool,
    },

    /// One HTTP/2 request whose bodies carry the stream (`type=http`).
    /// `host` is a comma-separated list, one picked per connection.
    H2 {
        #[serde(default)]
        path: String,
        #[serde(default)]
        host: String,
    },

    /// XHTTP (once SplitHTTP): the stream split over ordinary HTTP requests.
    Xhttp {
        #[serde(default)]
        path: String,
        #[serde(default)]
        host: String,
        #[serde(default)]
        mode: XhttpMode,
        /// The link's `extra` JSON, kept verbatim so a shared link carries
        /// it on; read when the session starts.
        #[serde(default)]
        extra: String,
    },
}

impl Transport {
    /// The share-link name, which the UI also keys its labels on.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::TcpHttp { .. } => "tcp-http",
            Self::Ws { .. } => "ws",
            Self::HttpUpgrade { .. } => "httpupgrade",
            Self::Grpc { .. } => "grpc",
            Self::H2 { .. } => "h2",
            Self::Xhttp { .. } => "xhttp",
        }
    }

    /// For the logs.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Tcp => "TCP",
            Self::TcpHttp { .. } => "TCP with an HTTP header",
            Self::Ws { .. } => "WebSocket",
            Self::HttpUpgrade { .. } => "HTTPUpgrade",
            Self::Grpc { .. } => "gRPC",
            Self::H2 { .. } => "HTTP/2",
            Self::Xhttp { .. } => "XHTTP",
        }
    }
}

/// How XHTTP splits the stream into requests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum XhttpMode {
    /// Whatever the client prefers - which is packet-up, as Xray's own is.
    #[default]
    Auto,
    /// A streaming download, and the upload as a series of POSTs.
    PacketUp,
    /// A streaming download and one streaming upload.
    StreamUp,
    /// Both directions in a single request.
    StreamOne,
}

impl XhttpMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::PacketUp => "packet-up",
            Self::StreamUp => "stream-up",
            Self::StreamOne => "stream-one",
        }
    }
}

/// The part of an outbound that shapes its TLS handshake - the same for every
/// protocol carried inside it.
#[derive(Debug, Clone, Copy)]
pub struct TlsSettings<'a> {
    pub sni: &'a str,
    pub alpn: &'a [String],
    pub allow_insecure: bool,
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
    /// Defaulted, like `transport`, so servers saved before either existed
    /// load as what they were: TLS over raw TCP.
    #[serde(default)]
    pub security: Security,
    #[serde(default)]
    pub transport: Transport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrojanConfig {
    /// The server's shared secret. Never logged, never returned to the UI.
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub sni: String,
    #[serde(default = "default_alpn")]
    pub alpn: Vec<String>,
    #[serde(default)]
    pub allow_insecure: bool,
    #[serde(default)]
    pub security: Security,
    #[serde(default)]
    pub transport: Transport,
}

pub fn default_alpn() -> Vec<String> {
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
            Outbound::Trojan(t) => !t.password.is_empty() && !self.host.is_empty(),
        }
    }
}

/// What a user may change about a server after adding it. Credentials are not
/// here on purpose: they never reach the UI, so they cannot be edited there -
/// a new key means importing the server's new link.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerEdit {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub sni: Option<String>,
    pub allow_insecure: Option<bool>,
}

impl ServerEdit {
    /// Whether applying this would change how the server is reached, as
    /// opposed to only what it is called.
    pub fn touches_connection(&self) -> bool {
        self.host.is_some()
            || self.port.is_some()
            || self.sni.is_some()
            || self.allow_insecure.is_some()
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
    pub port: u16,
    pub tier: Tier,
    pub load: u8,
    pub tags: Vec<String>,
    pub protocol: &'static str,
    /// [`Transport::kind`]; `None` for direct.
    pub transport: Option<&'static str>,
    /// Whether the connection is wrapped in TLS.
    pub tls: bool,
    /// The masking domain presented in the TLS handshake; empty for direct
    /// and for servers without TLS.
    /// Not a secret - it is visible on the wire to anyone watching.
    pub sni: String,
    pub allow_insecure: bool,
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
            port: node.port,
            tier: node.tier,
            load: node.load,
            tags: node.tags.clone(),
            protocol: node.outbound.protocol(),
            transport: node.outbound.transport().map(Transport::kind),
            tls: node.outbound.tls().is_some(),
            sni: node
                .outbound
                .tls()
                .map(|tls| tls.sni.to_string())
                .unwrap_or_default(),
            allow_insecure: node.outbound.tls().is_some_and(|tls| tls.allow_insecure),
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
    /// Routing locally with no remote, purely so traffic can be metered and
    /// attributed. Deliberately distinct from `Connected`: nothing is masked,
    /// and calling it protection would be a lie the user acts on.
    Monitoring,
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
    /// Keep routing through the local proxy while no VPN is connected, so the
    /// usage breakdown can name apps and hosts. Needs `system_proxy`.
    pub monitor_when_idle: bool,
    /// Sweep the local network every few minutes and say when a device joins.
    /// Defaulted on its own, so settings saved before it existed still load.
    #[serde(default = "default_true")]
    pub watch_devices: bool,
    /// Show live upload and download speed on the Windows taskbar.
    #[serde(default = "default_true")]
    pub taskbar_speed: bool,
    /// Ask GitHub now and then whether a newer release is out.
    #[serde(default = "default_true")]
    pub check_updates: bool,
    pub preferred_server: Option<String>,
}

fn default_true() -> bool {
    true
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
            monitor_when_idle: false,
            watch_devices: true,
            taskbar_speed: true,
            check_updates: true,
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
    pub monitor_when_idle: Option<bool>,
    pub watch_devices: Option<bool>,
    pub taskbar_speed: Option<bool>,
    pub check_updates: Option<bool>,
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
        if let Some(v) = patch.monitor_when_idle {
            self.monitor_when_idle = v;
        }
        if let Some(v) = patch.watch_devices {
            self.watch_devices = v;
        }
        if let Some(v) = patch.taskbar_speed {
            self.taskbar_speed = v;
        }
        if let Some(v) = patch.check_updates {
            self.check_updates = v;
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

// ---------------------------------------------------------------------------
// Updates
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    /// This build.
    pub current: String,
    /// The newest published release, once a check has succeeded.
    pub latest: Option<String>,
    /// `latest` is newer than `current`.
    pub available: bool,
    /// Unix seconds of the last successful check.
    pub checked_at: Option<u64>,
}
