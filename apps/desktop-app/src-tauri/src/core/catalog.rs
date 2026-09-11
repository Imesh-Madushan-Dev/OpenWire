//! Server catalog: bundled defaults, on-disk user catalog, and `vless://` import.
//!
//! Credentials live in the user's config directory, never in the repository and
//! never in the binary. The bundled defaults ship with empty UUIDs on purpose -
//! a node stays `configured: false` until the user supplies its access key.

use crate::core::models::{Outbound, ServerNode, VlessConfig};
use crate::error::{AppError, AppResult};

/// Seeded into the user's catalog on first launch.
///
/// `maizy-direct` is a diagnostic node: it proves the local proxy and system
/// routing work without needing any remote infrastructure.
pub const DEFAULT_CATALOG: &str = r#"[
  {
    "id": "maizy-direct",
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
  },
  {
    "id": "sg-01",
    "name": "Singapore",
    "country": "Singapore",
    "countryCode": "sg",
    "city": "Singapore",
    "host": "",
    "port": 443,
    "tier": "free",
    "outbound": { "kind": "vless", "uuid": "", "sni": "teams.microsoft.com" },
    "load": 38,
    "tags": ["low-latency"]
  },
  {
    "id": "de-01",
    "name": "Frankfurt",
    "country": "Germany",
    "countryCode": "de",
    "city": "Frankfurt",
    "host": "",
    "port": 443,
    "tier": "free",
    "outbound": { "kind": "vless", "uuid": "", "sni": "zoom.us" },
    "load": 52,
    "tags": ["streaming"]
  },
  {
    "id": "us-01",
    "name": "New York",
    "country": "United States",
    "countryCode": "us",
    "city": "New York",
    "host": "",
    "port": 443,
    "tier": "premium",
    "outbound": { "kind": "vless", "uuid": "", "sni": "teams.microsoft.com" },
    "load": 61,
    "tags": ["streaming", "dedicated"]
  },
  {
    "id": "jp-01",
    "name": "Tokyo",
    "country": "Japan",
    "countryCode": "jp",
    "city": "Tokyo",
    "host": "",
    "port": 443,
    "tier": "premium",
    "outbound": { "kind": "vless", "uuid": "", "sni": "meet.google.com" },
    "load": 24,
    "tags": ["gaming", "dedicated"]
  },
  {
    "id": "uk-01",
    "name": "London",
    "country": "United Kingdom",
    "countryCode": "gb",
    "city": "London",
    "host": "",
    "port": 443,
    "tier": "premium",
    "outbound": { "kind": "vless", "uuid": "", "sni": "zoom.us" },
    "load": 45,
    "tags": ["streaming"]
  }
]"#;

pub fn default_nodes() -> Vec<ServerNode> {
    // A parse failure here is a build-time authoring bug, not a runtime concern,
    // and the test below pins it.
    serde_json::from_str(DEFAULT_CATALOG).unwrap_or_default()
}

/// Picks the best node for "Optimal location": configured, lowest load, and
/// never the unmasked diagnostic node.
pub fn optimal<'a>(nodes: &'a [ServerNode], latency: impl Fn(&str) -> Option<u32>) -> Option<&'a ServerNode> {
    nodes
        .iter()
        .filter(|n| n.is_configured() && !matches!(n.outbound, Outbound::Direct))
        // Rank by measured latency when we have it, else fall back to advertised load.
        .min_by_key(|n| latency(&n.id).unwrap_or(500 + u32::from(n.load)))
}

// ---------------------------------------------------------------------------
// vless:// import
// ---------------------------------------------------------------------------

/// Parses `vless://uuid@host:port?security=tls&sni=example.com#Label`.
///
/// This is the share-link format every VLESS server emits, so it is the path a
/// user actually has to hand when adding their own node.
pub fn parse_vless_uri(uri: &str) -> AppResult<ServerNode> {
    let invalid = |msg: &str| AppError::Invalid(format!("That link isn't a valid server link: {msg}"));

    let rest = uri
        .trim()
        .strip_prefix("vless://")
        .ok_or_else(|| invalid("it must start with vless://"))?;

    // Split off the #label before anything else - labels may contain @ and ?.
    let (rest, label) = match rest.split_once('#') {
        Some((r, l)) => (r, percent_decode(l)),
        None => (rest, String::new()),
    };

    let (rest, query) = match rest.split_once('?') {
        Some((r, q)) => (r, q),
        None => (rest, ""),
    };

    let (uuid, endpoint) = rest
        .split_once('@')
        .ok_or_else(|| invalid("the access key is missing"))?;
    if uuid.is_empty() {
        return Err(invalid("the access key is empty"));
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
        .parse()
        .map_err(|_| invalid("the port must be a number"))?;

    let mut sni = String::new();
    let mut alpn = vec!["h2".to_string(), "http/1.1".to_string()];
    let mut allow_insecure = false;
    let mut security_tls = false;

    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = percent_decode(value);
        match key {
            "sni" | "host" | "peer" => {
                if sni.is_empty() {
                    sni = value;
                }
            }
            "security" => security_tls = matches!(value.as_str(), "tls" | "reality" | "xtls"),
            "alpn" => {
                let parsed: Vec<String> = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if !parsed.is_empty() {
                    alpn = parsed;
                }
            }
            "allowInsecure" | "insecure" => allow_insecure = value == "1" || value == "true",
            _ => {}
        }
    }

    if !security_tls && query.contains("security=") {
        return Err(invalid("only TLS links are supported"));
    }
    // Without an explicit SNI there is nothing to camouflage as; fall back to the
    // real host so the handshake at least stays valid.
    if sni.is_empty() {
        sni = host.to_string();
    }

    let name = if label.is_empty() {
        host.to_string()
    } else {
        label
    };

    Ok(ServerNode {
        id: format!("custom-{}", short_hash(&format!("{host}:{port}:{uuid}"))),
        name: name.clone(),
        country: "Custom".to_string(),
        country_code: "xx".to_string(),
        city: String::new(),
        host: host.to_string(),
        port,
        tier: crate::core::models::Tier::Free,
        outbound: Outbound::Vless(VlessConfig {
            uuid: uuid.to_string(),
            sni,
            alpn,
            allow_insecure,
        }),
        load: 0,
        tags: vec!["custom".to_string()],
    })
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

    #[test]
    fn bundled_catalog_parses() {
        let nodes = default_nodes();
        assert_eq!(nodes.len(), 6, "default catalog failed to parse");
        // The diagnostic node is the only one usable before setup.
        assert!(nodes[0].is_configured());
        assert!(!nodes[1].is_configured());
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

    #[test]
    fn same_link_yields_same_id() {
        let link = "vless://abc@1.2.3.4:443#One";
        let a = parse_vless_uri(link).unwrap();
        let b = parse_vless_uri(link).unwrap();
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn optimal_skips_unconfigured_and_direct() {
        let mut nodes = default_nodes();
        // Nothing is configured except the direct node, which is excluded.
        assert!(optimal(&nodes, |_| None).is_none());

        if let Outbound::Vless(v) = &mut nodes[2].outbound {
            v.uuid = "key".to_string();
        }
        nodes[2].host = "example.net".to_string();
        assert_eq!(optimal(&nodes, |_| None).map(|n| n.id.as_str()), Some("de-01"));
    }
}
