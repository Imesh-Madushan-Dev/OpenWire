//! How the tunnel service hands a flow to the app's local proxy.
//!
//! The proxy already speaks SOCKS5, and RFC 1928 reserves method numbers
//! 0x80-0xFE for private use. The service offers [`METHOD`] in its greeting;
//! once the proxy picks it, the rest of the exchange is OpenWire's own:
//!
//! ```text
//! service → proxy   05 01 8F                          greeting, private method only
//! proxy → service   05 8F                             accepted
//! service → proxy   01 net src dst                    version, 1=TCP/2=UDP, two addresses
//! proxy → service   01 status                         after the upstream leg is up
//! ```
//!
//! Each address is a SOCKS5 address type (1 = IPv4, 4 = IPv6), the address,
//! then a big-endian port. The source is the app's own end of the flow,
//! which is what lets usage be attributed to it.
//!
//! After that a TCP flow is raw bytes. A UDP flow is datagrams, each a
//! big-endian u16 length and the payload - exactly VLESS's UDP framing, so a
//! VLESS outbound carries it without re-encoding. One flow is one source and
//! one destination, like a connected UDP socket.
//!
//! A standard SOCKS5 client never offers this method, so nothing changes for
//! apps pointed at the proxy by hand.

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// From the SOCKS5 private-method range.
pub const METHOD: u8 = 0x8F;
pub const VERSION: u8 = 1;

/// The largest datagram a frame can describe; the length is a u16.
pub const MAX_DATAGRAM: usize = u16::MAX as usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Tcp,
    Udp,
}

impl Network {
    fn byte(self) -> u8 {
        match self {
            Self::Tcp => 1,
            Self::Udp => 2,
        }
    }

    fn from_byte(byte: u8) -> io::Result<Self> {
        match byte {
            1 => Ok(Self::Tcp),
            2 => Ok(Self::Udp),
            other => Err(invalid(format!("unknown network {other}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlowRequest {
    pub network: Network,
    /// The app's end of the flow, on the adapter.
    pub source: SocketAddr,
    /// Where it was going.
    pub destination: SocketAddr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    /// The upstream leg could not be opened.
    Failed,
    /// Refused on purpose: the kill switch is holding traffic.
    Blocked,
}

impl Status {
    fn byte(self) -> u8 {
        match self {
            Self::Ok => 0,
            Self::Failed => 1,
            Self::Blocked => 2,
        }
    }

    fn from_byte(byte: u8) -> Self {
        match byte {
            0 => Self::Ok,
            2 => Self::Blocked,
            _ => Self::Failed,
        }
    }
}

impl FlowRequest {
    /// The greeting and the request, in one write: the proxy reads exactly
    /// what each step needs, so there is no reason to wait between them.
    pub fn handshake(&self) -> Vec<u8> {
        let mut bytes = vec![0x05, 0x01, METHOD, VERSION, self.network.byte()];
        put_address(&mut bytes, self.source);
        put_address(&mut bytes, self.destination);
        bytes
    }

    /// Reads the request, once the proxy has accepted [`METHOD`].
    pub async fn read<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Self> {
        let mut head = [0u8; 2];
        reader.read_exact(&mut head).await?;
        if head[0] != VERSION {
            return Err(invalid(format!("unknown version {}", head[0])));
        }
        Ok(Self {
            network: Network::from_byte(head[1])?,
            source: read_address(reader).await?,
            destination: read_address(reader).await?,
        })
    }
}

pub fn status_reply(status: Status) -> [u8; 2] {
    [VERSION, status.byte()]
}

/// The proxy's answers, as the service reads them.
pub async fn read_acceptance<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Status> {
    let mut choice = [0u8; 2];
    reader.read_exact(&mut choice).await?;
    if choice != [0x05, METHOD] {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            "the local proxy does not take system-wide flows",
        ));
    }
    let mut reply = [0u8; 2];
    reader.read_exact(&mut reply).await?;
    if reply[0] != VERSION {
        return Err(invalid(format!("unknown version {}", reply[0])));
    }
    Ok(Status::from_byte(reply[1]))
}

fn put_address(bytes: &mut Vec<u8>, address: SocketAddr) {
    match address.ip() {
        IpAddr::V4(v4) => {
            bytes.push(0x01);
            bytes.extend_from_slice(&v4.octets());
        }
        IpAddr::V6(v6) => {
            bytes.push(0x04);
            bytes.extend_from_slice(&v6.octets());
        }
    }
    bytes.extend_from_slice(&address.port().to_be_bytes());
}

async fn read_address<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<SocketAddr> {
    let mut kind = [0u8; 1];
    reader.read_exact(&mut kind).await?;
    let ip = match kind[0] {
        0x01 => {
            let mut v4 = [0u8; 4];
            reader.read_exact(&mut v4).await?;
            IpAddr::V4(Ipv4Addr::from(v4))
        }
        0x04 => {
            let mut v6 = [0u8; 16];
            reader.read_exact(&mut v6).await?;
            IpAddr::V6(Ipv6Addr::from(v6))
        }
        other => return Err(invalid(format!("unknown address type {other}"))),
    };
    let mut port = [0u8; 2];
    reader.read_exact(&mut port).await?;
    Ok(SocketAddr::new(ip, u16::from_be_bytes(port)))
}

/// Length-prefixes one datagram.
pub fn frame(payload: &[u8]) -> io::Result<Vec<u8>> {
    let len = u16::try_from(payload.len()).map_err(|_| invalid("datagram too large".into()))?;
    let mut bytes = Vec::with_capacity(2 + payload.len());
    bytes.extend_from_slice(&len.to_be_bytes());
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

pub async fn write_datagram<W: AsyncWrite + Unpin>(
    writer: &mut W,
    payload: &[u8],
) -> io::Result<()> {
    writer.write_all(&frame(payload)?).await?;
    writer.flush().await
}

/// Reads one datagram into `buffer`. `Ok(false)` is a clean end of stream
/// between datagrams; ending inside one is an error.
pub async fn read_datagram<R: AsyncRead + Unpin>(
    reader: &mut R,
    buffer: &mut Vec<u8>,
) -> io::Result<bool> {
    let mut len = [0u8; 2];
    match reader.read_exact(&mut len).await {
        Ok(_) => {}
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(false),
        Err(err) => return Err(err),
    }
    buffer.resize(u16::from_be_bytes(len) as usize, 0);
    reader.read_exact(buffer).await?;
    Ok(true)
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_request_round_trips_through_its_own_reader() {
        let request = FlowRequest {
            network: Network::Udp,
            source: "172.19.0.1:50000".parse().unwrap(),
            destination: "[2606:4700::1111]:443".parse().unwrap(),
        };
        let bytes = request.handshake();
        assert_eq!(
            &bytes[..3],
            &[0x05, 0x01, METHOD],
            "a SOCKS5 greeting first"
        );

        let mut rest = &bytes[3..];
        assert_eq!(FlowRequest::read(&mut rest).await.unwrap(), request);
        assert!(rest.is_empty(), "nothing left over");
    }

    #[tokio::test]
    async fn garbage_is_refused_not_misread() {
        let mut wrong_version: &[u8] = &[9, 1];
        assert!(FlowRequest::read(&mut wrong_version).await.is_err());
        let mut wrong_network: &[u8] = &[VERSION, 7];
        assert!(FlowRequest::read(&mut wrong_network).await.is_err());
        let mut domain_address: &[u8] = &[VERSION, 1, 0x03, 1, b'a', 0, 80];
        assert!(FlowRequest::read(&mut domain_address).await.is_err());
    }

    #[tokio::test]
    async fn the_proxy_answer_is_read_as_a_status() {
        let mut ok: &[u8] = &[0x05, METHOD, VERSION, 0];
        assert_eq!(read_acceptance(&mut ok).await.unwrap(), Status::Ok);
        let mut blocked: &[u8] = &[0x05, METHOD, VERSION, 2];
        assert_eq!(
            read_acceptance(&mut blocked).await.unwrap(),
            Status::Blocked
        );
        // A proxy that does not know the method answers "no acceptable methods".
        let mut refused: &[u8] = &[0x05, 0xFF];
        assert!(read_acceptance(&mut refused).await.is_err());
    }

    #[tokio::test]
    async fn datagrams_keep_their_boundaries() {
        let mut stream = Vec::new();
        write_datagram(&mut stream, b"one").await.unwrap();
        write_datagram(&mut stream, b"").await.unwrap();
        write_datagram(&mut stream, b"three").await.unwrap();

        let mut reader = &stream[..];
        let mut buffer = Vec::new();
        for expected in [&b"one"[..], b"", b"three"] {
            assert!(read_datagram(&mut reader, &mut buffer).await.unwrap());
            assert_eq!(buffer, expected);
        }
        assert!(
            !read_datagram(&mut reader, &mut buffer).await.unwrap(),
            "clean end"
        );

        let mut torn: &[u8] = &[0, 5, b'a'];
        assert!(read_datagram(&mut torn, &mut buffer).await.is_err());
        assert!(frame(&vec![0; MAX_DATAGRAM + 1]).is_err());
    }
}
