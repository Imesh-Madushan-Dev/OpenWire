//! VLESS outbound, carried by whichever transport the server's link names -
//! see [`super::transport`].

use uuid::Uuid;

use super::transport::Dialer;
use super::IoStream;
use crate::core::models::VlessConfig;
use crate::error::{AppError, AppResult};

/// Opens the tunnel with the VLESS request head for `target` as its first bytes.
pub async fn dial(
    dialer: &Dialer,
    config: &VlessConfig,
    target_host: &str,
    target_port: u16,
) -> AppResult<Box<dyn IoStream>> {
    let uuid = Uuid::parse_str(&config.uuid).map_err(|_| {
        AppError::Invalid("The access key for this server is not a valid UUID.".into())
    })?;

    dialer
        .open(request_head(uuid.as_bytes(), target_host, target_port))
        .await
}

/// VLESS v0 request head:
/// `[ver=0][uuid:16][addon_len=0][cmd=1 tcp][port:u16be][atyp][addr]`
fn request_head(uuid: &[u8; 16], host: &str, port: u16) -> Vec<u8> {
    let mut head = Vec::with_capacity(32 + host.len());
    head.push(0x00); // version
    head.extend_from_slice(uuid);
    head.push(0x00); // no addons
    head.push(0x01); // TCP
    head.extend_from_slice(&port.to_be_bytes());

    // Note the address types are VLESS's, not SOCKS5's: 1=IPv4, 2=domain, 3=IPv6.
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(v4)) => {
            head.push(0x01);
            head.extend_from_slice(&v4.octets());
        }
        Ok(std::net::IpAddr::V6(v6)) => {
            head.push(0x03);
            head.extend_from_slice(&v6.octets());
        }
        Err(_) => {
            head.push(0x02);
            // Domain length is a single byte; longer names are not representable.
            let bytes = host.as_bytes();
            let len = bytes.len().min(255);
            head.push(len as u8);
            head.extend_from_slice(&bytes[..len]);
        }
    }
    head
}

/// Number of leading bytes of the VLESS response to discard:
/// `[ver][addon_len][addons...]`.
pub const RESPONSE_HEAD_MIN: usize = 2;

#[cfg(test)]
mod tests {
    use super::*;

    fn uuid_bytes() -> [u8; 16] {
        Uuid::parse_str("2b4f8c1e-0000-4000-8000-abcdefabcdef")
            .unwrap()
            .into_bytes()
    }

    #[test]
    fn domain_head_matches_the_vless_layout() {
        let head = request_head(&uuid_bytes(), "example.com", 443);

        assert_eq!(head[0], 0x00, "version");
        assert_eq!(&head[1..17], &uuid_bytes(), "uuid");
        assert_eq!(head[17], 0x00, "addon length");
        assert_eq!(head[18], 0x01, "tcp command");
        assert_eq!(&head[19..21], &443u16.to_be_bytes(), "port");
        assert_eq!(head[21], 0x02, "domain address type");
        assert_eq!(head[22], 11, "domain length");
        assert_eq!(&head[23..], b"example.com");
    }

    #[test]
    fn ip_literals_use_the_binary_address_types() {
        let v4 = request_head(&uuid_bytes(), "203.0.113.10", 8443);
        assert_eq!(v4[21], 0x01);
        assert_eq!(&v4[22..26], &[203, 0, 113, 10]);
        assert_eq!(v4.len(), 26);

        let v6 = request_head(&uuid_bytes(), "::1", 443);
        assert_eq!(v6[21], 0x03);
        assert_eq!(v6.len(), 38);
    }

    #[test]
    fn overlong_domains_are_truncated_not_panicking() {
        let long = "a".repeat(400);
        let head = request_head(&uuid_bytes(), &long, 443);
        assert_eq!(head[22], 255);
    }
}
