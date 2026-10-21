//! Where on earth an IP address is.
//!
//! Backed by DB-IP's country database, compiled into the binary and read in
//! place: no file to locate at runtime, nothing to parse up front, and no
//! lookup ever leaves the device - asking an online service would hand it a
//! list of every host the user reaches. A lookup walks a binary trie of at
//! most 128 nodes and decodes one two-letter string, well under a microsecond.
//! See `resources/geo/README.md` for the licence and how to update the data.

use std::net::IpAddr;
use std::sync::OnceLock;

use maxminddb::{PathElement, Reader};

static DATABASE: &[u8] = include_bytes!("../../resources/geo/dbip-country-lite.mmdb");

/// Opened on first use. `None` only if the embedded file is corrupt, which
/// costs flags rather than the app.
fn reader() -> Option<&'static Reader<&'static [u8]>> {
    static READER: OnceLock<Option<Reader<&'static [u8]>>> = OnceLock::new();
    READER
        .get_or_init(|| match Reader::from_source(DATABASE) {
            Ok(reader) => Some(reader),
            Err(err) => {
                log::warn!("geo database unreadable: {err}");
                None
            }
        })
        .as_ref()
}

const ISO_CODE: &[PathElement<'static>] = &[PathElement::Key("country"), PathElement::Key("iso_code")];

/// The ISO 3166-1 alpha-2 code (`"US"`) of the country an address is in.
///
/// `None` for addresses that are not on the public internet - this machine,
/// the LAN, link-local - and for anything the database does not place.
pub fn country(ip: IpAddr) -> Option<&'static str> {
    if !is_public(ip) {
        return None;
    }
    reader()?
        .lookup(ip)
        .ok()?
        .decode_path::<&'static str>(ISO_CODE)
        .ok()
        .flatten()
        .filter(|code| code.len() == 2)
}

/// The country of a host given as text, when the text is itself an address.
pub fn country_of_literal(host: &str) -> Option<&'static str> {
    // `[::1]` is how an IPv6 literal arrives in a URL authority.
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    bare.parse().ok().and_then(country)
}

fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                // 100.64.0.0/10, carrier-grade NAT.
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // fc00::/7 unique local, fe80::/10 link-local.
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_addresses_resolve_to_a_two_letter_code() {
        // Google's and Cloudflare's resolvers: stable, well-known allocations.
        assert_eq!(country("8.8.8.8".parse().unwrap()).map(str::len), Some(2));
        assert_eq!(country_of_literal("1.1.1.1").map(str::len), Some(2));
        assert_eq!(country_of_literal("[2606:4700:4700::1111]").map(str::len), Some(2));
    }

    #[test]
    fn private_and_non_address_hosts_have_no_country() {
        for host in ["192.168.1.1", "10.0.0.2", "127.0.0.1", "::1", "fe80::1", "100.64.1.1", "example.com"] {
            assert_eq!(country_of_literal(host), None, "{host}");
        }
    }
}
