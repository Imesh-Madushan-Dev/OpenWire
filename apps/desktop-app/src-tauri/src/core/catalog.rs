//! Server catalog: the on-disk user catalog, and share-link import and export
//! for the protocols this client speaks (`vless://`, `trojan://`).
//!
//! Credentials live in the user's config directory, never in the repository and
//! never in the binary. The bundled defaults ship with empty UUIDs on purpose -
//! a node stays `configured: false` until the user supplies its access key.

use rustls::pki_types::ServerName;

use crate::core::models::{
    Outbound, Security, ServerEdit, ServerNode, Transport, TrojanConfig, VlessConfig, XhttpMode,
};
use crate::core::tunnel::transport;
use crate::error::{AppError, AppResult};

/// The local route monitor mode and the diagnostic option run through.
///
/// Not a server: there is no remote, the proxy dials destinations itself. It is
/// deliberately NOT seeded into the user's catalog - nothing ships pre-added,
/// and this is resolved by id when something needs it.
pub const DIRECT_NODE: &str = r#"{
  "id": "openwire-direct",
  "name": "Direct (diagnostic)",
  "country": "Local",
  "countryCode": "xx",
  "city": "This device",
  "host": "127.0.0.1",
  "port": 0,
  "tier": "free",
  "outbound": { "kind": "direct" },
  "load": 0,
  "tags": ["diagnostic", "unmasked"]
}"#;

/// The local route, or `None` if this file was mis-edited.
pub fn direct_node() -> Option<ServerNode> {
    serde_json::from_str(DIRECT_NODE).ok()
}

/// Drops entries a user never added and cannot use.
///
/// Earlier builds seeded six placeholder locations with empty credentials. They
/// are already on disk for existing installs, where they show as "Setup
/// required" rows that can never be set up - there is no key to add, because
/// there was never a server behind them. An unconfigured node is meaningless
/// now that nothing ships pre-added, so this clears them out on load.
pub fn strip_unusable(nodes: &mut Vec<ServerNode>) -> usize {
    let before = nodes.len();
    nodes.retain(ServerNode::is_configured);
    before - nodes.len()
}

/// Picks the best node for "Optimal location": configured, lowest load, and
/// never the unmasked diagnostic node.
pub fn optimal<'a>(
    nodes: &'a [ServerNode],
    latency: impl Fn(&str) -> Option<u32>,
) -> Option<&'a ServerNode> {
    nodes
        .iter()
        .filter(|n| n.is_configured() && !matches!(n.outbound, Outbound::Direct))
        // Rank by measured latency when we have it, else fall back to advertised load.
        .min_by_key(|n| latency(&n.id).unwrap_or(500 + u32::from(n.load)))
}

// ---------------------------------------------------------------------------
// Share-link import
// ---------------------------------------------------------------------------

/// Schemes recognised in pasted text, supported or not. Unsupported ones are
/// still picked out so the import can say "2 links skipped" instead of
/// silently finding nothing.
const LINK_SCHEMES: [&str; 11] = [
    "vless",
    "trojan",
    "vmess",
    "ss",
    "ssr",
    "socks",
    "hysteria2",
    "hy2",
    "tuic",
    "wireguard",
    "wg",
];

/// Parses any supported share link into a node.
pub fn parse_share_link(uri: &str) -> AppResult<ServerNode> {
    let trimmed = uri.trim();
    let scheme = trimmed
        .split_once("://")
        .map(|(scheme, _)| scheme.to_ascii_lowercase())
        .unwrap_or_default();

    match scheme.as_str() {
        "vless" => parse_vless_uri(trimmed),
        "trojan" => parse_trojan_uri(trimmed),
        "" => Err(AppError::Invalid(
            "That isn't a server link. Links start with vless:// or trojan://.".into(),
        )),
        other => Err(AppError::Invalid(format!(
            "{other}:// links aren't supported yet. OpenWire speaks VLESS and Trojan."
        ))),
    }
}

/// Every share link in a block of text: a pasted list, a file, a decoded QR
/// code, or a base64 subscription body - the format subscription URLs serve.
pub fn extract_links(text: &str) -> Vec<String> {
    let found = scan_links(text);
    if !found.is_empty() {
        return found;
    }
    decode_subscription(text)
        .map(|decoded| scan_links(&decoded))
        .unwrap_or_default()
}

fn scan_links(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter(|token| {
            token.split_once("://").is_some_and(|(scheme, rest)| {
                !rest.is_empty() && LINK_SCHEMES.contains(&scheme.to_ascii_lowercase().as_str())
            })
        })
        .map(str::to_string)
        .collect()
}

fn decode_subscription(text: &str) -> Option<String> {
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
    use base64::Engine;

    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() < 16 {
        return None;
    }
    [STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD]
        .iter()
        .find_map(|engine| engine.decode(&compact).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .filter(|decoded| decoded.contains("://"))
}

/// The pieces every supported link shares:
/// `scheme://secret@host:port?key=value&...#label`.
struct LinkParts {
    secret: String,
    host: String,
    port: u16,
    label: String,
    sni: String,
    alpn: Vec<String>,
    allow_insecure: bool,
    security: Security,
    transport: Transport,
}

fn invalid(msg: &str) -> AppError {
    AppError::Invalid(format!("That link isn't a valid server link: {msg}"))
}

fn parse_parts(uri: &str, scheme: &str, secret_name: &str) -> AppResult<LinkParts> {
    let prefix = format!("{scheme}://");
    let rest = uri
        .get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(&prefix))
        .map(|_| &uri[prefix.len()..])
        .ok_or_else(|| invalid(&format!("it must start with {prefix}")))?;

    // Split off the #label before anything else - labels may contain @ and ?.
    let (rest, label) = match rest.split_once('#') {
        Some((r, l)) => (r, percent_decode(l)),
        None => (rest, String::new()),
    };

    let (rest, query) = match rest.split_once('?') {
        Some((r, q)) => (r, q),
        None => (rest, ""),
    };

    // rsplit: a Trojan password may itself contain an `@`.
    let (secret, endpoint) = rest
        .rsplit_once('@')
        .ok_or_else(|| invalid(&format!("the {secret_name} is missing")))?;
    let secret = percent_decode(secret);
    if secret.is_empty() {
        return Err(invalid(&format!("the {secret_name} is empty")));
    }

    // rsplit so IPv6 literals in brackets survive.
    let (host, port) = endpoint
        .rsplit_once(':')
        .ok_or_else(|| invalid("the port is missing"))?;
    let host = host.trim_matches(|c| c == '[' || c == ']');
    if host.is_empty() {
        return Err(invalid("the host is empty"));
    }
    let port: u16 = port
        .trim_end_matches('/')
        .parse()
        .map_err(|_| invalid("the port must be a number"))?;

    let params: Vec<(&str, String)> = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (key, percent_decode(value))
        })
        .collect();
    // The first value given for a key, or "" when there is none.
    let param = |key: &str| {
        params
            .iter()
            .find(|(k, _)| *k == key)
            .map_or("", |(_, value)| value.as_str())
    };

    // Reject what this client cannot actually speak, rather than importing a
    // node that would fail later with a confusing handshake error. REALITY and
    // XTLS in particular look like TLS in the link but need a different
    // handshake entirely.
    let security = match param("security") {
        "" | "tls" => Security::Tls,
        "none" => Security::None,
        other => {
            return Err(invalid(&format!(
                "`{other}` isn't supported yet - this client speaks plain TLS, or no TLS"
            )))
        }
    };
    // A server expecting XTLS Vision refuses a client without it, and VLESS
    // encryption is a handshake of its own; neither is spoken here.
    let flow = param("flow");
    if !flow.is_empty() {
        return Err(invalid(&format!("the `{flow}` flow isn't supported yet")));
    }
    if !matches!(param("encryption"), "" | "none") {
        return Err(invalid("encrypted VLESS isn't supported yet"));
    }
    let transport = parse_transport(&param)?;

    // `host` is the HTTP Host header for the HTTP transports; a fallback name
    // only when nothing more specific was given. Without any SNI there is
    // nothing to camouflage as, so the real host keeps the handshake valid.
    let first_host = param("host").split(',').next().unwrap_or("").trim();
    let sni = [param("sni"), param("peer"), first_host, host]
        .into_iter()
        .find(|name| !name.is_empty())
        .unwrap_or(host)
        .to_string();

    let alpn: Vec<String> = param("alpn")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let insecure = [param("allowInsecure"), param("insecure")];

    Ok(LinkParts {
        secret,
        host: host.to_string(),
        port,
        label,
        sni,
        alpn: if alpn.is_empty() {
            crate::core::models::default_alpn()
        } else {
            alpn
        },
        allow_insecure: insecure.iter().any(|v| *v == "1" || *v == "true"),
        security,
        transport,
    })
}

/// The transport a link's `type` names, with its settings.
fn parse_transport<'a>(param: &impl Fn(&str) -> &'a str) -> AppResult<Transport> {
    let path = param("path").to_string();
    let host = param("host").to_string();

    Ok(match param("type") {
        // Xray has been renaming `tcp` to `raw`.
        "" | "tcp" | "raw" => match param("headerType") {
            "" | "none" => Transport::Tcp,
            "http" => Transport::TcpHttp { host, path },
            other => {
                return Err(invalid(&format!(
                    "the `{other}` TCP header isn't supported yet"
                )))
            }
        },
        "ws" | "websocket" => {
            let (path, early_data) = split_early_data(&path);
            Transport::Ws {
                path,
                host,
                early_data,
            }
        }
        // `ed` here only tells Xray not to wait for the upgrade's answer;
        // this client always waits, so it has no use for it.
        "httpupgrade" => Transport::HttpUpgrade {
            path: split_early_data(&path).0,
            host,
        },
        "grpc" | "gun" => Transport::Grpc {
            service_name: param("serviceName").to_string(),
            authority: param("authority").to_string(),
            multi: param("mode") == "multi",
        },
        "http" | "h2" => Transport::H2 { path, host },
        "xhttp" | "splithttp" => {
            let mode = match param("mode") {
                "" | "auto" => XhttpMode::Auto,
                "packet-up" => XhttpMode::PacketUp,
                "stream-up" => XhttpMode::StreamUp,
                "stream-one" => XhttpMode::StreamOne,
                other => return Err(invalid(&format!("the XHTTP mode `{other}` isn't known"))),
            };
            let extra = param("extra").to_string();
            transport::check_xhttp_extra(&extra)?;
            Transport::Xhttp {
                path,
                host,
                mode,
                extra,
            }
        }
        kind @ ("kcp" | "mkcp" | "quic") => {
            return Err(invalid(&format!(
                "`{kind}` runs over UDP, which this client doesn't tunnel yet"
            )))
        }
        other => {
            return Err(invalid(&format!(
                "`{other}` transport isn't supported yet"
            )))
        }
    })
}

/// Splits Xray's `ed` - early-data bytes - out of a path's query, where links
/// carry it: `/ws?ed=2048` is the path `/ws` with 2048 bytes.
fn split_early_data(path: &str) -> (String, usize) {
    let Some((base, query)) = path.split_once('?') else {
        return (path.to_string(), 0);
    };
    let mut early_data = 0;
    let rest: Vec<&str> = query
        .split('&')
        .filter(|pair| match pair.strip_prefix("ed=") {
            Some(bytes) => {
                early_data = bytes.parse().unwrap_or(0);
                false
            }
            None => !pair.is_empty(),
        })
        .collect();
    let path = if rest.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{}", rest.join("&"))
    };
    (path, early_data)
}

/// The inverse of [`split_early_data`].
fn join_early_data(path: &str, early_data: usize) -> String {
    match (early_data, path.contains('?')) {
        (0, _) => path.to_string(),
        (_, true) => format!("{path}&ed={early_data}"),
        (_, false) => format!("{path}?ed={early_data}"),
    }
}

fn custom_node(id_seed: &str, parts: &LinkParts, outbound: Outbound) -> ServerNode {
    let name = if parts.label.is_empty() {
        parts.host.clone()
    } else {
        parts.label.clone()
    };

    ServerNode {
        id: format!("custom-{}", short_hash(id_seed)),
        name,
        country: "Custom".to_string(),
        country_code: "xx".to_string(),
        city: String::new(),
        host: parts.host.clone(),
        port: parts.port,
        tier: crate::core::models::Tier::Free,
        outbound,
        load: 0,
        tags: vec!["custom".to_string()],
    }
}

/// Parses `vless://uuid@host:port?security=tls&sni=example.com#Label`.
pub fn parse_vless_uri(uri: &str) -> AppResult<ServerNode> {
    let parts = parse_parts(uri.trim(), "vless", "access key")?;
    // Seeded without a scheme: this is the id every existing VLESS node
    // already has, and changing it would duplicate them on re-import.
    let seed = format!("{}:{}:{}", parts.host, parts.port, parts.secret);
    let outbound = Outbound::Vless(VlessConfig {
        uuid: parts.secret.clone(),
        sni: parts.sni.clone(),
        alpn: parts.alpn.clone(),
        allow_insecure: parts.allow_insecure,
        security: parts.security,
        transport: parts.transport.clone(),
    });
    Ok(custom_node(&seed, &parts, outbound))
}

/// Parses `trojan://password@host:port?security=tls&sni=example.com#Label`.
/// The password is percent-decoded: share links escape anything outside the
/// URL-safe set.
pub fn parse_trojan_uri(uri: &str) -> AppResult<ServerNode> {
    let parts = parse_parts(uri.trim(), "trojan", "password")?;
    let seed = format!("trojan:{}:{}:{}", parts.host, parts.port, parts.secret);
    let outbound = Outbound::Trojan(TrojanConfig {
        password: parts.secret.clone(),
        sni: parts.sni.clone(),
        alpn: parts.alpn.clone(),
        allow_insecure: parts.allow_insecure,
        security: parts.security,
        transport: parts.transport.clone(),
    });
    Ok(custom_node(&seed, &parts, outbound))
}

// ---------------------------------------------------------------------------
// Share-link export
// ---------------------------------------------------------------------------

/// The share link for a node - the same format it was imported from, so it
/// can be pasted into this or any other client. `None` for the direct route,
/// which has nothing to share.
pub fn share_link(node: &ServerNode) -> Option<String> {
    let (scheme, secret, sni, alpn, security, transport) = match &node.outbound {
        Outbound::Direct => return None,
        Outbound::Vless(c) => ("vless", &c.uuid, &c.sni, &c.alpn, c.security, &c.transport),
        Outbound::Trojan(c) => ("trojan", &c.password, &c.sni, &c.alpn, c.security, &c.transport),
    };

    let host = if node.host.contains(':') {
        format!("[{}]", node.host)
    } else {
        node.host.clone()
    };

    let mut query = Vec::new();
    if scheme == "vless" {
        query.push("encryption=none".to_string());
    }
    // The SNI and ALPN go along even without TLS, so the link imports back
    // to exactly this server.
    query.push(match security {
        Security::Tls => "security=tls".to_string(),
        Security::None => "security=none".to_string(),
    });
    query.push(format!("sni={}", percent_encode(sni)));
    query.push(format!("alpn={}", percent_encode(&alpn.join(","))));
    query.extend(transport_params(transport));
    if node.outbound.tls().is_some_and(|tls| tls.allow_insecure) {
        query.push("allowInsecure=1".to_string());
    }

    Some(format!(
        "{scheme}://{}@{host}:{}?{}#{}",
        percent_encode(secret),
        node.port,
        query.join("&"),
        percent_encode(&node.name)
    ))
}

/// `type=` and the settings that go with it, as [`parse_transport`] reads them.
fn transport_params(transport: &Transport) -> Vec<String> {
    let kind = match transport {
        Transport::Tcp | Transport::TcpHttp { .. } => "tcp",
        Transport::Ws { .. } => "ws",
        Transport::HttpUpgrade { .. } => "httpupgrade",
        Transport::Grpc { .. } => "grpc",
        Transport::H2 { .. } => "http",
        Transport::Xhttp { .. } => "xhttp",
    };
    let mut query = vec![format!("type={kind}")];
    let mut push = |key: &str, value: &str| {
        if !value.is_empty() {
            query.push(format!("{key}={}", percent_encode(value)));
        }
    };

    match transport {
        Transport::Tcp => {}
        Transport::TcpHttp { host, path } => {
            push("headerType", "http");
            push("host", host);
            push("path", path);
        }
        Transport::Ws {
            path,
            host,
            early_data,
        } => {
            push("path", &join_early_data(path, *early_data));
            push("host", host);
        }
        Transport::HttpUpgrade { path, host } | Transport::H2 { path, host } => {
            push("path", path);
            push("host", host);
        }
        Transport::Grpc {
            service_name,
            authority,
            multi,
        } => {
            push("serviceName", service_name);
            push("authority", authority);
            push("mode", if *multi { "multi" } else { "gun" });
        }
        Transport::Xhttp {
            path,
            host,
            mode,
            extra,
        } => {
            push("path", path);
            push("host", host);
            push("mode", mode.as_str());
            push("extra", extra);
        }
    }
    query
}

// ---------------------------------------------------------------------------
// Editing
// ---------------------------------------------------------------------------

/// Applies a user's edit, validating every field it touches. On error the
/// node is left exactly as it was.
pub fn apply_edit(node: &mut ServerNode, edit: ServerEdit) -> AppResult<()> {
    let bad = |msg: &str| AppError::Invalid(msg.to_string());
    let mut next = node.clone();

    if let Some(name) = edit.name {
        let name = name.trim();
        if name.is_empty() {
            return Err(bad("Give the server a name."));
        }
        next.name = name.chars().take(64).collect();
    }

    if let Some(host) = edit.host {
        let host = host.trim().trim_matches(|c| c == '[' || c == ']');
        if host.is_empty() || host.contains(|c: char| c.is_whitespace() || c == '/') {
            return Err(bad(
                "Enter the server's address, like vpn.example.com or 203.0.113.10.",
            ));
        }
        next.host = host.to_string();
    }

    if let Some(port) = edit.port {
        if port == 0 {
            return Err(bad("Choose a port between 1 and 65535."));
        }
        next.port = port;
    }

    let tls = match &mut next.outbound {
        Outbound::Direct => {
            return if edit.sni.is_some() || edit.allow_insecure.is_some() {
                Err(bad(
                    "The direct route has no connection settings to change.",
                ))
            } else {
                *node = next;
                Ok(())
            };
        }
        Outbound::Vless(config) => (&mut config.sni, &mut config.allow_insecure, config.security),
        Outbound::Trojan(config) => (&mut config.sni, &mut config.allow_insecure, config.security),
    };
    if tls.2 == Security::None && (edit.sni.is_some() || edit.allow_insecure.is_some()) {
        return Err(bad(
            "This server doesn't use TLS, so it has no masking domain or certificate check to change.",
        ));
    }

    if let Some(sni) = edit.sni {
        let sni = sni.trim();
        // The handshake will reject anything that is not a valid server name,
        // so reject it here, where the user can still fix it.
        if sni.is_empty() || ServerName::try_from(sni.to_string()).is_err() {
            return Err(bad(
                "The masking domain must be a hostname, like www.microsoft.com.",
            ));
        }
        *tls.0 = sni.to_string();
    }
    if let Some(allow_insecure) = edit.allow_insecure {
        *tls.1 = allow_insecure;
    }

    *node = next;
    Ok(())
}

/// Percent-encodes everything outside RFC 3986's unreserved set.
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Deterministic short id so re-importing the same link updates rather than
/// duplicates. FNV-1a - not a security hash, just a stable label.
fn short_hash(input: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:x}")[..8].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINK: &str = "vless://2b4f8c1e-0000-4000-8000-abcdefabcdef@203.0.113.10:443?security=tls&sni=zoom.us&type=tcp#Edge";

    /// A node as a user would add it: from a share link, fully configured.
    fn imported() -> ServerNode {
        parse_vless_uri(LINK).expect("fixture link should parse")
    }

    /// The shape earlier builds seeded: a location with no credentials behind it.
    fn placeholder(id: &str) -> ServerNode {
        let mut node = imported();
        node.id = id.to_string();
        if let Outbound::Vless(vless) = &mut node.outbound {
            vless.uuid.clear();
        }
        node
    }

    #[test]
    fn the_direct_route_parses_and_needs_no_credentials() {
        // Monitor mode resolves this by id; it is no longer in any catalog.
        let direct = direct_node().expect("the direct route must parse");
        assert!(direct.is_configured());
        assert!(matches!(direct.outbound, Outbound::Direct));
    }

    #[test]
    fn placeholder_servers_from_older_builds_are_stripped_on_load() {
        let mut nodes = vec![placeholder("sg-01"), imported(), placeholder("uk-01")];

        assert_eq!(strip_unusable(&mut nodes), 2);
        assert_eq!(nodes.len(), 1, "the user's own server must survive");
        assert!(nodes.iter().all(ServerNode::is_configured));
    }

    #[test]
    fn parses_a_real_share_link() {
        let node = parse_vless_uri(
            "vless://2b4f8c1e-0000-4000-8000-abcdefabcdef@203.0.113.10:443?encryption=none&security=tls&sni=teams.microsoft.com&type=tcp#Tokyo%20Edge",
        )
        .expect("should parse");

        assert_eq!(node.name, "Tokyo Edge");
        assert_eq!(node.host, "203.0.113.10");
        assert_eq!(node.port, 443);
        assert!(node.is_configured());
        match node.outbound {
            Outbound::Vless(v) => {
                // The camouflage name must differ from the real endpoint.
                assert_eq!(v.sni, "teams.microsoft.com");
                assert!(!v.allow_insecure, "insecure must not be on by default");
            }
            _ => panic!("expected vless outbound"),
        }
    }

    #[test]
    fn rejects_junk() {
        assert!(parse_vless_uri("https://example.com").is_err());
        assert!(parse_vless_uri("vless://no-at-sign").is_err());
        assert!(parse_vless_uri("vless://uuid@host").is_err());
        assert!(parse_vless_uri("vless://uuid@host:notaport").is_err());
    }

    /// A real share link: the decoy SNI differs from the endpoint, and the
    /// percent-encoded ALPN list and label both have to survive decoding.
    #[test]
    fn parses_a_masked_link_with_a_decoy_sni() {
        let node = parse_vless_uri(
            "vless://2b4f8c1e-0000-4000-8000-abcdefabcdef@edge.example.org:443?alpn=h2%2Chttp%2F1.1&encryption=none&fp=chrome&security=tls&sni=aka.ms&type=tcp#MY%20NODE-imesh",
        )
        .expect("should parse");

        assert_eq!(node.name, "MY NODE-imesh");
        assert_eq!(node.host, "edge.example.org");
        match node.outbound {
            Outbound::Vless(v) => {
                assert_eq!(v.sni, "aka.ms", "decoy must not collapse into the host");
                assert_eq!(v.alpn, vec!["h2".to_string(), "http/1.1".to_string()]);
                assert!(!v.allow_insecure);
            }
            _ => panic!("expected vless outbound"),
        }
    }

    /// Importing a node this client cannot speak must fail at import time, not
    /// with a mystery handshake error on first connect.
    #[test]
    fn rejects_unsupported_transports_and_security() {
        for uri in [
            "vless://key@host:443?security=reality&pbk=abc&type=tcp",
            "vless://key@host:443?security=xtls&type=tcp",
            "vless://key@host:443?security=tls&type=tcp&flow=xtls-rprx-vision",
            "vless://key@host:443?security=tls&type=tcp&encryption=mlkem768x25519plus.native.0rtt.abc",
            "vless://key@host:443?security=tls&type=kcp",
            "vless://key@host:443?security=tls&type=tcp&headerType=srtp",
            "vless://key@host:443?security=tls&type=xhttp&mode=stream-sideways",
            "vless://key@host:443?security=tls&type=xhttp&extra=%7B%22downloadSettings%22%3A%7B%7D%7D",
        ] {
            let err = parse_vless_uri(uri).expect_err("should reject");
            assert_eq!(err.code(), "INVALID_INPUT", "{uri}");
        }

        for uri in [
            "vless://key@host:443?security=tls&type=tcp",
            "vless://key@host:443?security=tls&type=ws&path=%2Fws",
            "vless://key@host:443?security=tls&type=grpc",
            "vless://key@host:80?security=none&type=httpupgrade",
        ] {
            assert!(parse_vless_uri(uri).is_ok(), "{uri}");
        }
    }

    fn transport_of(uri: &str) -> (Transport, Security) {
        match parse_share_link(uri).expect(uri).outbound {
            Outbound::Vless(v) => (v.transport, v.security),
            Outbound::Trojan(t) => (t.transport, t.security),
            Outbound::Direct => unreachable!(),
        }
    }

    #[test]
    fn every_transport_reads_its_own_settings() {
        assert_eq!(
            transport_of("vless://k@h:443?type=ws&path=%2Fws%3Fed%3D2048&host=cdn.example.com"),
            (
                Transport::Ws {
                    path: "/ws".into(),
                    host: "cdn.example.com".into(),
                    early_data: 2048,
                },
                Security::Tls
            )
        );
        assert_eq!(
            transport_of("trojan://p@h:80?security=none&type=httpupgrade&path=%2Fup%3Fed%3D2048"),
            (
                Transport::HttpUpgrade {
                    path: "/up".into(),
                    host: String::new(),
                },
                Security::None
            )
        );
        assert_eq!(
            transport_of("vless://k@h:443?type=grpc&serviceName=my%20svc&mode=multi&authority=a.example"),
            (
                Transport::Grpc {
                    service_name: "my svc".into(),
                    authority: "a.example".into(),
                    multi: true,
                },
                Security::Tls
            )
        );
        assert_eq!(
            transport_of("vless://k@h:443?type=xhttp&path=%2Fxh&mode=stream-one").0,
            Transport::Xhttp {
                path: "/xh".into(),
                host: String::new(),
                mode: XhttpMode::StreamOne,
                extra: String::new(),
            }
        );
        assert!(matches!(
            transport_of("vless://k@h:443?type=splithttp").0,
            Transport::Xhttp { mode: XhttpMode::Auto, .. }
        ));
        assert_eq!(
            transport_of("vless://k@h:80?type=tcp&headerType=http&host=a.com%2Cb.com&path=%2F").0,
            Transport::TcpHttp {
                host: "a.com,b.com".into(),
                path: "/".into(),
            }
        );
        assert_eq!(transport_of("vless://k@h:443?type=raw").0, Transport::Tcp);
        assert!(matches!(
            transport_of("vless://k@h:443?type=h2&path=%2Fh2").0,
            Transport::H2 { .. }
        ));
    }

    #[test]
    fn the_host_header_stands_in_for_a_missing_sni() {
        let node = parse_vless_uri("vless://k@203.0.113.10:443?type=ws&host=cdn.example.com").unwrap();
        assert_eq!(node.outbound.tls().unwrap().sni, "cdn.example.com");

        let node = parse_vless_uri("vless://k@203.0.113.10:443?sni=aka.ms&type=ws&host=cdn.example.com")
            .unwrap();
        assert_eq!(node.outbound.tls().unwrap().sni, "aka.ms");
    }

    #[test]
    fn early_data_moves_between_the_path_and_the_link() {
        assert_eq!(split_early_data("/ws"), ("/ws".to_string(), 0));
        assert_eq!(split_early_data("/ws?ed=2048"), ("/ws".to_string(), 2048));
        assert_eq!(split_early_data("/ws?a=1&ed=80&b=2"), ("/ws?a=1&b=2".to_string(), 80));
        assert_eq!(join_early_data("/ws", 2048), "/ws?ed=2048");
        assert_eq!(join_early_data("/ws?a=1", 80), "/ws?a=1&ed=80");
        assert_eq!(join_early_data("/ws", 0), "/ws");
    }

    /// Catalogs saved before transports existed have no such fields; they
    /// must still load, as the TLS-over-TCP servers they were.
    #[test]
    fn servers_saved_before_transports_load_as_tls_over_tcp() {
        let saved = r#"{"kind":"vless","uuid":"k","sni":"aka.ms","alpn":["h2"],"allowInsecure":false}"#;
        let outbound: Outbound = serde_json::from_str(saved).unwrap();
        match outbound {
            Outbound::Vless(v) => {
                assert_eq!(v.transport, Transport::Tcp);
                assert_eq!(v.security, Security::Tls);
            }
            other => panic!("expected vless, got {other:?}"),
        }
    }

    #[test]
    fn parses_a_trojan_link_with_an_escaped_password() {
        let node = parse_trojan_uri(
            "trojan://p%40ss%3Aword@edge.example.org:443?security=tls&sni=aka.ms&type=tcp&headerType=none#Trojan%20One",
        )
        .expect("should parse");

        assert_eq!(node.name, "Trojan One");
        assert_eq!(node.host, "edge.example.org");
        assert_eq!(node.port, 443);
        assert!(node.is_configured());
        match node.outbound {
            Outbound::Trojan(t) => {
                assert_eq!(t.password, "p@ss:word");
                assert_eq!(t.sni, "aka.ms");
                assert!(!t.allow_insecure);
            }
            other => panic!("expected trojan, got {other:?}"),
        }
    }

    #[test]
    fn trojan_rejects_what_this_client_cannot_speak() {
        assert!(parse_trojan_uri("trojan://p@host:443?type=quic").is_err());
        assert!(parse_trojan_uri("trojan://p@host:443?security=reality").is_err());
        assert!(
            parse_trojan_uri("trojan://@host:443").is_err(),
            "empty password"
        );
        assert!(
            parse_trojan_uri("trojan://p@host:443").is_ok(),
            "tls is the default"
        );
    }

    #[test]
    fn the_scheme_picks_the_parser_and_names_what_is_unsupported() {
        assert!(matches!(
            parse_share_link(LINK).unwrap().outbound,
            Outbound::Vless(_)
        ));
        assert!(matches!(
            parse_share_link("TROJAN://p@h:443").unwrap().outbound,
            Outbound::Trojan(_)
        ));

        let err = parse_share_link("vmess://eyJ2IjoiMiJ9").unwrap_err();
        assert!(
            err.user_message().contains("vmess://"),
            "{}",
            err.user_message()
        );
        assert!(parse_share_link("hello").is_err());
    }

    #[test]
    fn links_are_found_in_pasted_text_and_in_subscriptions() {
        let pasted = format!(
            "My servers:\n{LINK}\n  trojan://p@h:443#B  \nvmess://abc\nhttps://example.com"
        );
        let links = extract_links(&pasted);
        assert_eq!(links.len(), 3, "{links:?}");
        assert!(
            links.iter().any(|l| l.starts_with("vmess://")),
            "kept so it can be reported"
        );

        use base64::Engine;
        let subscription = base64::engine::general_purpose::STANDARD
            .encode(format!("{LINK}\ntrojan://p@h:443#B\n"));
        assert_eq!(extract_links(&subscription).len(), 2);
        assert!(extract_links("not a link and not base64!").is_empty());
    }

    #[test]
    fn a_shared_link_imports_back_to_the_same_server() {
        for link in [
            "vless://2b4f8c1e-0000-4000-8000-abcdefabcdef@[2001:db8::1]:8443?security=tls&sni=aka.ms&alpn=h2&type=tcp&allowInsecure=1#Edge%20%231",
            "trojan://p%40ss@edge.example.org:443?security=tls&sni=zoom.us#Trojan%20%C3%A9",
            "vless://k@edge.example.org:443?security=tls&sni=aka.ms&type=ws&path=%2Fws%3Fx%3D1%26ed%3D2048&host=cdn.example.com#WS",
            "vless://k@edge.example.org:80?security=none&type=ws&path=%2Fws&host=cdn.example.com#Plain",
            "trojan://p@edge.example.org:443?type=grpc&serviceName=svc&mode=multi&authority=a.example#gRPC",
            "vless://k@edge.example.org:443?type=httpupgrade&path=%2Fup&host=cdn.example.com#Up",
            "vless://k@edge.example.org:443?type=http&path=%2Fh2&host=a.example%2Cb.example#H2",
            "vless://k@edge.example.org:443?type=xhttp&path=%2Fxh&mode=packet-up&extra=%7B%22xPaddingBytes%22%3A%22100-200%22%7D#XHTTP",
            "vless://k@edge.example.org:80?security=none&type=tcp&headerType=http&host=a.example&path=%2Fp#Fake",
        ] {
            let node = parse_share_link(link).unwrap();
            let shared = share_link(&node).expect("shareable");
            let again = parse_share_link(&shared).unwrap();
            assert_eq!(again, node, "{shared}");
        }
        assert!(share_link(&direct_node().unwrap()).is_none());
    }

    #[test]
    fn edits_validate_and_leave_the_node_alone_on_error() {
        let mut node = imported();
        let before = node.clone();

        let bad = ServerEdit {
            name: Some("Renamed".into()),
            sni: Some("not a hostname!".into()),
            ..ServerEdit::default()
        };
        assert!(apply_edit(&mut node, bad).is_err());
        assert_eq!(node, before, "a rejected edit must change nothing");

        let good = ServerEdit {
            name: Some("  Renamed  ".into()),
            port: Some(8443),
            sni: Some("www.microsoft.com".into()),
            allow_insecure: Some(true),
            ..ServerEdit::default()
        };
        apply_edit(&mut node, good).expect("valid edit");
        assert_eq!(node.name, "Renamed");
        assert_eq!(node.port, 8443);
        assert_eq!(node.id, before.id, "ids stay stable across edits");
        match &node.outbound {
            Outbound::Vless(v) => {
                assert_eq!(v.sni, "www.microsoft.com");
                assert!(v.allow_insecure);
                assert_eq!(
                    v.uuid, "2b4f8c1e-0000-4000-8000-abcdefabcdef",
                    "secret untouched"
                );
            }
            other => panic!("expected vless, got {other:?}"),
        }
    }

    #[test]
    fn a_server_without_tls_has_no_masking_settings_to_edit() {
        let mut node = parse_vless_uri("vless://k@edge.example.org:80?security=none&type=ws").unwrap();
        assert!(node.outbound.tls().is_none());
        let edit = ServerEdit {
            sni: Some("www.microsoft.com".into()),
            ..ServerEdit::default()
        };
        assert!(apply_edit(&mut node, edit).is_err());
        let rename = ServerEdit {
            name: Some("Port 80".into()),
            ..ServerEdit::default()
        };
        assert!(apply_edit(&mut node, rename).is_ok());
    }

    #[test]
    fn same_link_yields_same_id() {
        let link = "vless://abc@1.2.3.4:443#One";
        let a = parse_vless_uri(link).unwrap();
        let b = parse_vless_uri(link).unwrap();
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn optimal_skips_unconfigured_and_direct() {
        let mut nodes = vec![direct_node().unwrap(), placeholder("sg-01")];
        // The only usable node is the unmasked direct route, which never counts.
        assert!(optimal(&nodes, |_| None).is_none());

        let mut usable = imported();
        usable.id = "de-01".to_string();
        nodes.push(usable);
        assert_eq!(
            optimal(&nodes, |_| None).map(|n| n.id.as_str()),
            Some("de-01")
        );
    }
}
