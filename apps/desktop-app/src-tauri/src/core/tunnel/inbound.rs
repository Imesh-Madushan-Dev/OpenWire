//! Local proxy front end.
//!
//! One listener speaks both SOCKS5 and HTTP proxy, chosen by sniffing the first
//! byte (SOCKS5 always starts `0x05`). That matters on Windows: WinINET drives
//! most apps through the HTTP proxy, while everything else prefers SOCKS5.

use std::net::Ipv6Addr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::error::{AppError, AppResult};

/// Cap on the request head we will buffer, so a hostile local client cannot
/// make us allocate without bound.
const MAX_HEAD: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Socks5,
    HttpConnect,
    HttpPlain,
}

#[derive(Debug)]
pub struct Request {
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    /// Bytes already read from the client that belong upstream (the rewritten
    /// request line for plain HTTP, plus any pipelined body).
    pub prefix: Vec<u8>,
}

/// Reads just enough to know where the client wants to go.
pub async fn negotiate(stream: &mut TcpStream) -> AppResult<Request> {
    let mut first = [0u8; 1];
    let n = stream
        .peek(&mut first)
        .await
        .map_err(|e| AppError::Protocol(format!("peek: {e}")))?;
    if n == 0 {
        return Err(AppError::Protocol("client closed before sending".into()));
    }

    if first[0] == 0x05 {
        socks5_negotiate(stream).await
    } else {
        http_negotiate(stream).await
    }
}

/// Acknowledges the client once the upstream leg is up (or has failed).
pub async fn send_reply(stream: &mut TcpStream, protocol: Protocol, ok: bool) -> AppResult<()> {
    let bytes: &[u8] = match (protocol, ok) {
        // BND.ADDR/BND.PORT are zeroed: clients ignore them for CONNECT.
        (Protocol::Socks5, true) => &[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0],
        (Protocol::Socks5, false) => &[0x05, 0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0],
        (Protocol::HttpConnect, true) => {
            b"HTTP/1.1 200 Connection Established\r\nProxy-Agent: Maizy\r\n\r\n"
        }
        (Protocol::HttpConnect, false) => {
            b"HTTP/1.1 502 Bad Gateway\r\nProxy-Agent: Maizy\r\nContent-Length: 0\r\n\r\n"
        }
        // Plain HTTP has no separate acknowledgement; the origin's own response
        // is the reply. On failure we still owe the client a status.
        (Protocol::HttpPlain, true) => return Ok(()),
        (Protocol::HttpPlain, false) => {
            b"HTTP/1.1 502 Bad Gateway\r\nProxy-Agent: Maizy\r\nContent-Length: 0\r\n\r\n"
        }
    };

    stream
        .write_all(bytes)
        .await
        .map_err(|e| AppError::Protocol(format!("reply: {e}")))
}

// ---------------------------------------------------------------------------
// SOCKS5 (RFC 1928)
// ---------------------------------------------------------------------------

async fn socks5_negotiate(stream: &mut TcpStream) -> AppResult<Request> {
    let proto = |msg: String| AppError::Protocol(msg);

    let mut head = [0u8; 2];
    stream
        .read_exact(&mut head)
        .await
        .map_err(|e| proto(format!("socks greeting: {e}")))?;

    let mut methods = vec![0u8; head[1] as usize];
    stream
        .read_exact(&mut methods)
        .await
        .map_err(|e| proto(format!("socks methods: {e}")))?;

    // "No authentication" - safe because the listener is loopback-only.
    stream
        .write_all(&[0x05, 0x00])
        .await
        .map_err(|e| proto(format!("socks method reply: {e}")))?;

    let mut req = [0u8; 4];
    stream
        .read_exact(&mut req)
        .await
        .map_err(|e| proto(format!("socks request: {e}")))?;

    if req[1] != 0x01 {
        // 0x07 = command not supported (BIND / UDP ASSOCIATE).
        let _ = stream
            .write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
            .await;
        return Err(proto(format!("unsupported socks command {}", req[1])));
    }

    let host = match req[3] {
        0x01 => {
            let mut v4 = [0u8; 4];
            stream
                .read_exact(&mut v4)
                .await
                .map_err(|e| proto(format!("socks ipv4: {e}")))?;
            std::net::Ipv4Addr::from(v4).to_string()
        }
        0x03 => {
            let mut len = [0u8; 1];
            stream
                .read_exact(&mut len)
                .await
                .map_err(|e| proto(format!("socks domain len: {e}")))?;
            let mut domain = vec![0u8; len[0] as usize];
            stream
                .read_exact(&mut domain)
                .await
                .map_err(|e| proto(format!("socks domain: {e}")))?;
            String::from_utf8(domain).map_err(|_| proto("socks domain is not utf-8".into()))?
        }
        0x04 => {
            let mut v6 = [0u8; 16];
            stream
                .read_exact(&mut v6)
                .await
                .map_err(|e| proto(format!("socks ipv6: {e}")))?;
            Ipv6Addr::from(v6).to_string()
        }
        other => return Err(proto(format!("unsupported socks address type {other}"))),
    };

    let mut port = [0u8; 2];
    stream
        .read_exact(&mut port)
        .await
        .map_err(|e| proto(format!("socks port: {e}")))?;

    Ok(Request {
        protocol: Protocol::Socks5,
        host,
        port: u16::from_be_bytes(port),
        prefix: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// HTTP proxy
// ---------------------------------------------------------------------------

async fn http_negotiate(stream: &mut TcpStream) -> AppResult<Request> {
    let proto = |msg: String| AppError::Protocol(msg);

    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let head_end = loop {
        let n = stream
            .read(&mut chunk)
            .await
            .map_err(|e| proto(format!("http read: {e}")))?;
        if n == 0 {
            return Err(proto("client closed during http request".into()));
        }
        buf.extend_from_slice(&chunk[..n]);

        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            break pos + 4;
        }
        if buf.len() > MAX_HEAD {
            return Err(proto("http request head too large".into()));
        }
    };

    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_ascii_uppercase();
    let target = parts.next().unwrap_or_default();
    let version = parts.next().unwrap_or("HTTP/1.1");

    if method.is_empty() || target.is_empty() {
        return Err(proto("malformed http request line".into()));
    }

    if method == "CONNECT" {
        let (host, port) = split_authority(target, 443)?;
        return Ok(Request {
            protocol: Protocol::HttpConnect,
            host,
            port,
            // Anything after the head on a CONNECT is already tunnelled payload.
            prefix: buf[head_end..].to_vec(),
        });
    }

    // Absolute-form request (`GET http://host/path HTTP/1.1`). Strip the scheme
    // and authority so the origin server sees a normal origin-form request.
    let (authority, path) = match target.strip_prefix("http://") {
        Some(rest) => match rest.find('/') {
            Some(idx) => (&rest[..idx], &rest[idx..]),
            None => (rest, "/"),
        },
        None => {
            // Origin-form without a proxy authority: fall back to the Host header.
            let host_header = lines
                .find(|l| l.to_ascii_lowercase().starts_with("host:"))
                .and_then(|l| l.split_once(':').map(|(_, v)| v.trim()))
                .ok_or_else(|| proto("http request has no host".into()))?;
            (host_header, target)
        }
    };

    let (host, port) = split_authority(authority, 80)?;

    let mut prefix = format!("{method} {path} {version}\r\n").into_bytes();
    // Proxy hop-by-hop headers must not be forwarded to the origin.
    for line in head[request_line.len() + 2..].split("\r\n") {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("proxy-connection:") || lower.starts_with("proxy-authorization:") {
            continue;
        }
        prefix.extend_from_slice(line.as_bytes());
        prefix.extend_from_slice(b"\r\n");
    }
    prefix.extend_from_slice(&buf[head_end..]);

    Ok(Request {
        protocol: Protocol::HttpPlain,
        host,
        port,
        prefix,
    })
}

fn split_authority(authority: &str, default_port: u16) -> AppResult<(String, u16)> {
    // Bracketed IPv6 literal: `[::1]:443`.
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, tail) = rest
            .split_once(']')
            .ok_or_else(|| AppError::Protocol("malformed ipv6 authority".into()))?;
        let port = tail
            .strip_prefix(':')
            .map(str::parse::<u16>)
            .transpose()
            .map_err(|_| AppError::Protocol("malformed port".into()))?
            .unwrap_or(default_port);
        return Ok((host.to_string(), port));
    }

    match authority.rsplit_once(':') {
        Some((host, port)) => {
            let port = port
                .parse()
                .map_err(|_| AppError::Protocol("malformed port".into()))?;
            Ok((host.to_string(), port))
        }
        None => Ok((authority.to_string(), default_port)),
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    async fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client = tokio::spawn(async move { TcpStream::connect(addr).await.unwrap() });
        let (server, _) = listener.accept().await.unwrap();
        (client.await.unwrap(), server)
    }

    #[tokio::test]
    async fn parses_socks5_domain_connect() {
        let (mut client, mut server) = pair().await;
        tokio::spawn(async move {
            // Greeting, then CONNECT example.com:443.
            client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
            let mut ack = [0u8; 2];
            client.read_exact(&mut ack).await.unwrap();
            assert_eq!(ack, [0x05, 0x00]);

            let mut req = vec![0x05, 0x01, 0x00, 0x03, 11];
            req.extend_from_slice(b"example.com");
            req.extend_from_slice(&443u16.to_be_bytes());
            client.write_all(&req).await.unwrap();
        });

        let req = negotiate(&mut server).await.unwrap();
        assert_eq!(req.protocol, Protocol::Socks5);
        assert_eq!(req.host, "example.com");
        assert_eq!(req.port, 443);
        assert!(req.prefix.is_empty());
    }

    #[tokio::test]
    async fn parses_http_connect() {
        let (mut client, mut server) = pair().await;
        tokio::spawn(async move {
            client
                .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
                .await
                .unwrap();
        });

        let req = negotiate(&mut server).await.unwrap();
        assert_eq!(req.protocol, Protocol::HttpConnect);
        assert_eq!(req.host, "example.com");
        assert_eq!(req.port, 443);
    }

    #[tokio::test]
    async fn rewrites_absolute_form_to_origin_form() {
        let (mut client, mut server) = pair().await;
        tokio::spawn(async move {
            client
                .write_all(
                    b"GET http://neverssl.com/news HTTP/1.1\r\nHost: neverssl.com\r\nProxy-Connection: keep-alive\r\n\r\n",
                )
                .await
                .unwrap();
        });

        let req = negotiate(&mut server).await.unwrap();
        assert_eq!(req.protocol, Protocol::HttpPlain);
        assert_eq!(req.host, "neverssl.com");
        assert_eq!(req.port, 80);

        let forwarded = String::from_utf8(req.prefix).unwrap();
        assert!(
            forwarded.starts_with("GET /news HTTP/1.1\r\n"),
            "{forwarded}"
        );
        assert!(forwarded.contains("Host: neverssl.com"));
        assert!(
            !forwarded.to_ascii_lowercase().contains("proxy-connection"),
            "hop-by-hop header leaked upstream"
        );
    }

    #[test]
    fn authority_parsing_handles_ipv6_and_defaults() {
        assert_eq!(
            split_authority("example.com", 80).unwrap(),
            ("example.com".to_string(), 80)
        );
        assert_eq!(
            split_authority("[::1]:8443", 443).unwrap(),
            ("::1".to_string(), 8443)
        );
        assert!(split_authority("example.com:notaport", 80).is_err());
    }
}
