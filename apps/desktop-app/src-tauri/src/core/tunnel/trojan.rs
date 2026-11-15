//! Trojan outbound, carried by whichever transport the server's link names -
//! see [`super::transport`].
//!
//! Trojan proves the client knows the server's password by sending its
//! SHA-224 digest as hex, then names the destination SOCKS5-style. Nothing
//! comes back before the destination's own bytes: unlike VLESS there is no
//! response head to strip, so the relay starts streaming immediately.
//!
//! <https://trojan-gfw.github.io/trojan/protocol>
//!
//! UDP is Trojan's UDP ASSOCIATE (command 3): after the same head, every
//! datagram carries its own address - `[atyp][addr][port][len:u16be] CRLF
//! [payload]` - both ways. The tunnel service's flows are one destination
//! each, so the address going up is always the flow's, and the one coming
//! back is dropped.

use std::io;

use sha2::{Digest, Sha224};
use tokio::io::{AsyncRead, AsyncReadExt};

use super::transport::Dialer;
use super::IoStream;
use crate::core::models::TrojanConfig;
use crate::error::{AppError, AppResult};

const CONNECT: u8 = 0x01;
const UDP_ASSOCIATE: u8 = 0x03;

/// Opens the tunnel with the Trojan request head for `target` as its first bytes.
pub async fn dial(
    dialer: &Dialer,
    config: &TrojanConfig,
    target_host: &str,
    target_port: u16,
) -> AppResult<Box<dyn IoStream>> {
    open(dialer, config, CONNECT, target_host, target_port).await
}

/// The same, for UDP to `target`; datagrams then go through [`udp_packet`]
/// and [`read_udp_packet`].
pub async fn dial_udp(
    dialer: &Dialer,
    config: &TrojanConfig,
    target_host: &str,
    target_port: u16,
) -> AppResult<Box<dyn IoStream>> {
    open(dialer, config, UDP_ASSOCIATE, target_host, target_port).await
}

async fn open(
    dialer: &Dialer,
    config: &TrojanConfig,
    command: u8,
    target_host: &str,
    target_port: u16,
) -> AppResult<Box<dyn IoStream>> {
    if config.password.is_empty() {
        return Err(AppError::Invalid(
            "This server has no password. Add it again from its share link.".into(),
        ));
    }

    dialer
        .open(request_head(&config.password, command, target_host, target_port))
        .await
}

/// One datagram for the server, in Trojan's UDP framing.
pub fn udp_packet(host: &str, port: u16, payload: &[u8]) -> io::Result<Vec<u8>> {
    let len = u16::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "datagram too large"))?;
    let mut packet = Vec::with_capacity(1 + 16 + 2 + 2 + 2 + payload.len());
    put_address(&mut packet, host);
    packet.extend_from_slice(&port.to_be_bytes());
    packet.extend_from_slice(&len.to_be_bytes());
    packet.extend_from_slice(b"\r\n");
    packet.extend_from_slice(payload);
    Ok(packet)
}

/// Reads one datagram from the server into `payload`, dropping the address
/// it names. `Ok(false)` is a clean end of stream between datagrams.
pub async fn read_udp_packet<R: AsyncRead + Unpin>(
    reader: &mut R,
    payload: &mut Vec<u8>,
) -> io::Result<bool> {
    let mut kind = [0u8; 1];
    match reader.read_exact(&mut kind).await {
        Ok(_) => {}
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(false),
        Err(err) => return Err(err),
    }
    let address_len = match kind[0] {
        0x01 => 4,
        0x04 => 16,
        0x03 => {
            let mut len = [0u8; 1];
            reader.read_exact(&mut len).await?;
            len[0] as usize
        }
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown address type {other}"),
            ))
        }
    };
    // Address, port, length and CRLF, read and set aside in one go.
    let mut fixed = vec![0u8; address_len + 2 + 2 + 2];
    reader.read_exact(&mut fixed).await?;
    let len = u16::from_be_bytes([fixed[address_len + 2], fixed[address_len + 3]]) as usize;
    payload.resize(len, 0);
    reader.read_exact(payload).await?;
    Ok(true)
}

/// `hex(SHA224(password)) CRLF [cmd][atyp][addr][port:u16be] CRLF`
fn request_head(password: &str, command: u8, host: &str, port: u16) -> Vec<u8> {
    let digest = Sha224::digest(password.as_bytes());

    let mut head = Vec::with_capacity(56 + 2 + 4 + host.len() + 4);
    for byte in digest {
        head.extend_from_slice(format!("{byte:02x}").as_bytes());
    }
    head.extend_from_slice(b"\r\n");
    head.push(command);
    put_address(&mut head, host);
    head.extend_from_slice(&port.to_be_bytes());
    head.extend_from_slice(b"\r\n");
    head
}

/// Address types are SOCKS5's: 1 = IPv4, 3 = domain, 4 = IPv6.
fn put_address(head: &mut Vec<u8>, host: &str) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_password_travels_as_its_sha224_hex_digest() {
        let head = request_head("password", CONNECT, "example.com", 443);

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
        let head = request_head("p", CONNECT, "example.com", 443);
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
        let v4 = request_head("p", CONNECT, "203.0.113.10", 8443);
        assert_eq!(v4[59], 0x01);
        assert_eq!(&v4[60..64], &[203, 0, 113, 10]);
        assert_eq!(&v4[64..66], &8443u16.to_be_bytes());

        let v6 = request_head("p", CONNECT, "::1", 443);
        assert_eq!(v6[59], 0x04);
        assert_eq!(v6.len(), 58 + 1 + 1 + 16 + 2 + 2);
    }

    #[tokio::test]
    async fn udp_datagrams_carry_their_address_and_come_back_without_it() {
        let packet = udp_packet("203.0.113.5", 3478, b"stun").unwrap();
        assert_eq!(packet[0], 0x01);
        assert_eq!(&packet[1..5], &[203, 0, 113, 5]);
        assert_eq!(&packet[5..7], &3478u16.to_be_bytes());
        assert_eq!(&packet[7..9], &4u16.to_be_bytes());
        assert_eq!(&packet[9..11], b"\r\n");
        assert_eq!(&packet[11..], b"stun");

        // Two back to back, as the server sends them.
        let mut stream = packet.clone();
        stream.extend(udp_packet("2001:db8::1", 53, b"").unwrap());
        let mut reader = &stream[..];
        let mut payload = Vec::new();
        assert!(read_udp_packet(&mut reader, &mut payload).await.unwrap());
        assert_eq!(payload, b"stun");
        assert!(read_udp_packet(&mut reader, &mut payload).await.unwrap());
        assert!(payload.is_empty());
        assert!(!read_udp_packet(&mut reader, &mut payload).await.unwrap(), "clean end");

        let udp_head = request_head("p", UDP_ASSOCIATE, "203.0.113.5", 3478);
        assert_eq!(udp_head[58], 0x03, "UDP ASSOCIATE");
    }

    #[test]
    fn overlong_domains_are_truncated_not_panicking() {
        let head = request_head("p", CONNECT, &"a".repeat(400), 443);
        assert_eq!(head[60], 255);
    }
}
