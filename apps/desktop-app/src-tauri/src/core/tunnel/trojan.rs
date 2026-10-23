//! Trojan outbound, carried by whichever transport the server's link names -
//! see [`super::transport`].
//!
//! Trojan proves the client knows the server's password by sending its
//! SHA-224 digest as hex, then names the destination SOCKS5-style. Nothing
//! comes back before the destination's own bytes: unlike VLESS there is no
//! response head to strip, so the relay starts streaming immediately.
//!
//! <https://trojan-gfw.github.io/trojan/protocol>

use sha2::{Digest, Sha224};

use super::transport::Dialer;
use super::IoStream;
use crate::core::models::TrojanConfig;
use crate::error::{AppError, AppResult};

/// Opens the tunnel with the Trojan request head for `target` as its first bytes.
pub async fn dial(
    dialer: &Dialer,
    config: &TrojanConfig,
    target_host: &str,
    target_port: u16,
) -> AppResult<Box<dyn IoStream>> {
    if config.password.is_empty() {
        return Err(AppError::Invalid(
            "This server has no password. Add it again from its share link.".into(),
        ));
    }

    dialer
        .open(request_head(&config.password, target_host, target_port))
        .await
}

/// `hex(SHA224(password)) CRLF [cmd=1 connect][atyp][addr][port:u16be] CRLF`
///
/// Address types are SOCKS5's: 1 = IPv4, 3 = domain, 4 = IPv6.
fn request_head(password: &str, host: &str, port: u16) -> Vec<u8> {
    let digest = Sha224::digest(password.as_bytes());

    let mut head = Vec::with_capacity(56 + 2 + 4 + host.len() + 4);
    for byte in digest {
        head.extend_from_slice(format!("{byte:02x}").as_bytes());
    }
    head.extend_from_slice(b"\r\n");
    head.push(0x01); // CONNECT

    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(v4)) => {
            head.push(0x01);
            head.extend_from_slice(&v4.octets());
        }
        Ok(std::net::IpAddr::V6(v6)) => {
            head.push(0x04);
            head.extend_from_slice(&v6.octets());
        }
        Err(_) => {
            head.push(0x03);
            // Domain length is a single byte; longer names are not representable.
            let bytes = host.as_bytes();
            let len = bytes.len().min(255);
            head.push(len as u8);
            head.extend_from_slice(&bytes[..len]);
        }
    }

    head.extend_from_slice(&port.to_be_bytes());
    head.extend_from_slice(b"\r\n");
    head
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_password_travels_as_its_sha224_hex_digest() {
        let head = request_head("password", "example.com", 443);

        // SHA-224("password"), a published test vector.
        assert_eq!(
            &head[..56],
            b"d63dc919e201d7bc4c825630d2cf25fdc93d4b2f0d46706d29038d01"
        );
        assert_eq!(&head[56..58], b"\r\n");
        assert!(
            !head.windows(8).any(|w| w == b"password"),
            "the plain password must never be written"
        );
    }

    #[test]
    fn a_domain_target_uses_the_socks5_layout() {
        let head = request_head("p", "example.com", 443);
        let request = &head[58..];

        assert_eq!(request[0], 0x01, "connect");
        assert_eq!(request[1], 0x03, "domain address type");
        assert_eq!(request[2], 11, "domain length");
        assert_eq!(&request[3..14], b"example.com");
        assert_eq!(&request[14..16], &443u16.to_be_bytes(), "port");
        assert_eq!(&request[16..], b"\r\n");
    }

    #[test]
    fn ip_literals_use_the_binary_address_types() {
        let v4 = request_head("p", "203.0.113.10", 8443);
        assert_eq!(v4[59], 0x01);
        assert_eq!(&v4[60..64], &[203, 0, 113, 10]);
        assert_eq!(&v4[64..66], &8443u16.to_be_bytes());

        let v6 = request_head("p", "::1", 443);
        assert_eq!(v6[59], 0x04);
        assert_eq!(v6.len(), 58 + 1 + 1 + 16 + 2 + 2);
    }

    #[test]
    fn overlong_domains_are_truncated_not_panicking() {
        let head = request_head("p", &"a".repeat(400), 443);
        assert_eq!(head[60], 255);
    }
}
