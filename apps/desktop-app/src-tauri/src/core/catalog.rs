//! Server catalog: bundled defaults, on-disk user catalog, and `vless://` import.
//!
//! Credentials live in the user's config directory, never in the repository and
//! never in the binary. The bundled defaults ship with empty UUIDs on purpose -
//! a node stays `configured: false` until the user supplies its access key.

use crate::core::models::{Outbound, ServerNode, VlessConfig};
use crate::error::{AppError, AppResult};

/// The local route monitor mode and the diagnostic option run through.
///
/// Not a server: there is no remote, the proxy dials destinations itself. It is
/// deliberately NOT seeded into the user's catalog - nothing ships pre-added,
/// and this is resolved by id when something needs it.
pub const DIRECT_NODE: &str = r#"{
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

/// What a fresh install starts with: nothing. Servers are added by hand, from
/// a `vless://` share link the user pastes in.
pub fn default_nodes() -> Vec<ServerNode> {
    Vec::new()
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
    let mut security = String::new();
    let mut transport = String::new();

    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = percent_decode(value);
        match key {
            "sni" | "host" | "peer" => {
                if sni.is_empty() {
                    sni = value;
                }
            }
            "security" => security = value,
            "type" => transport = value,
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

    // Reject what this client cannot actually speak, rather than importing a
    // node that would fail later with a confusing handshake error. REALITY and
    // XTLS in particular look like TLS in the link but need a different
    // handshake entirely.
    if !security.is_empty() && security != "tls" {
        return Err(invalid(&format!(
            "`{security}` isn't supported yet - this client speaks plain TLS"
        )));
    }
    if !transport.is_empty() && transport != "tcp" {
        return Err(invalid(&format!(
            "`{transport}` transport isn't supported yet - this client speaks raw TCP"
        )));
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
    fn nothing_ships_pre_added_but_the_direct_route_still_parses() {
        assert!(default_nodes().is_empty(), "no server may be seeded on first launch");

        // Monitor mode resolves this by id, so it has to survive the removal.
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
            "vless://key@host:443?security=tls&type=ws&path=%2Fws",
            "vless://key@host:443?security=tls&type=grpc",
        ] {
            let err = parse_vless_uri(uri).expect_err("should reject");
            assert_eq!(err.code(), "INVALID_INPUT", "{uri}");
        }

        // Plain TLS over raw TCP is the supported combination.
        assert!(parse_vless_uri("vless://key@host:443?security=tls&type=tcp").is_ok());
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
        assert_eq!(optimal(&nodes, |_| None).map(|n| n.id.as_str()), Some("de-01"));
    }
}
