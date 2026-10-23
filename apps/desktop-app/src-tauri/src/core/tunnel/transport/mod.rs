//! Transports: how a protocol's bytes reach the server once a connection is
//! open - written straight down it, or framed inside HTTP so the server can
//! sit behind a CDN or a web server.
//!
//! VLESS and Trojan only ever produce a request head and then a byte stream.
//! The [`Dialer`] carries both, whichever framing the share link asked for,
//! and the layers always stack the same way:
//!
//!   TCP -> TLS, unless the link said `security=none` -> the transport.
//!
//! Every transport hands back one [`IoStream`], so the relay never needs to
//! know which it got.

mod http1;
mod http2;
#[cfg(test)]
mod interop;
mod ws;
mod xhttp;

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{ready, Context, Poll};
use std::time::Duration;

use rustls::ClientConfig;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::time::timeout;

use super::{tls, IoStream};
use crate::core::models::{ServerNode, TlsSettings, Transport};
use crate::error::{AppError, AppResult};

/// Bounds what comes after TCP and TLS, which time out on their own: an
/// upgrade or a first response that never arrives.
const OPEN_TIMEOUT: Duration = Duration::from_secs(20);

/// The ALPN a TLS handshake has to offer for `transport` to work over it.
/// WebSocket and HTTP upgrades only exist in HTTP/1.1, and gRPC and the h2
/// transport only in HTTP/2, so those override the link - as Xray does.
pub fn alpn(transport: &Transport, configured: &[String]) -> Vec<String> {
    let only = |protocol: &str| vec![protocol.to_string()];
    match transport {
        Transport::Tcp | Transport::TcpHttp { .. } => configured.to_vec(),
        Transport::Ws { .. } | Transport::HttpUpgrade { .. } => only("http/1.1"),
        Transport::Grpc { .. } | Transport::H2 { .. } => only("h2"),
        // HTTP/1.1 when that is all the link allows, HTTP/2 otherwise. A
        // link asking for h3 gets HTTP/2: QUIC is not something this speaks.
        Transport::Xhttp { .. } if http1_only(configured) => only("http/1.1"),
        Transport::Xhttp { .. } => only("h2"),
    }
}

fn http1_only(alpn: &[String]) -> bool {
    alpn.len() == 1 && alpn[0] == "http/1.1"
}

/// Refuses XHTTP settings this client cannot follow, for the importer to
/// reject a link with before it is ever saved.
pub fn check_xhttp_extra(extra: &str) -> AppResult<()> {
    xhttp::Settings::parse(extra).map(drop)
}

/// Opens connections to one server, framed the way its share link says.
/// Built once per session: the TLS config and the shared HTTP/2 connection
/// live here, not in each connection.
pub struct Dialer {
    endpoint: Endpoint,
    transport: Transport,
    /// Read once from the link's `extra`, for XHTTP.
    xhttp: Option<xhttp::Settings>,
    /// Whether XHTTP runs over HTTP/2 rather than HTTP/1.1: over TLS unless
    /// the link pins HTTP/1.1, never without TLS - again as Xray decides it.
    xhttp_h2: bool,
    h2: http2::Pool,
}

impl Dialer {
    pub fn new(node: &ServerNode) -> AppResult<Self> {
        let transport =
            node.outbound.transport().cloned().ok_or_else(|| {
                AppError::Internal("the direct route has no server to dial".into())
            })?;

        let tls = node
            .outbound
            .tls()
            .map(|settings| {
                let alpn = alpn(&transport, settings.alpn);
                Ok::<_, AppError>(Tls {
                    config: tls::build(
                        TlsSettings {
                            alpn: &alpn,
                            ..settings
                        },
                        &node.host,
                    )?,
                    sni: settings.sni.to_string(),
                    http1_only: http1_only(&alpn),
                })
            })
            .transpose()?;
        let xhttp_h2 = tls.as_ref().is_some_and(|tls| !tls.http1_only);

        let xhttp = match &transport {
            Transport::Xhttp { extra, .. } => Some(xhttp::Settings::parse(extra)?),
            _ => None,
        };

        Ok(Self {
            endpoint: Endpoint {
                host: node.host.clone(),
                port: node.port,
                tls,
            },
            transport,
            xhttp,
            xhttp_h2,
            h2: http2::Pool::default(),
        })
    }

    /// A stream to the server whose first bytes are `head`, the protocol's
    /// request head.
    pub async fn open(&self, head: Vec<u8>) -> AppResult<Box<dyn IoStream>> {
        timeout(OPEN_TIMEOUT, self.open_framed(head))
            .await
            .map_err(|_| {
                AppError::Dial(format!("{} handshake timed out", self.transport.label()))
            })?
    }

    async fn open_framed(&self, head: Vec<u8>) -> AppResult<Box<dyn IoStream>> {
        let endpoint = &self.endpoint;
        // Written ahead of the head, in the same write.
        let mut preamble = Vec::new();
        // How much of the head the transport has already sent itself.
        let mut sent = 0;

        let mut stream: Box<dyn IoStream> = match &self.transport {
            Transport::Tcp => endpoint.connect().await?,
            Transport::TcpHttp { host, path } => {
                preamble = http1::request(
                    "GET",
                    &path_or_root(pick(path).unwrap_or("/")),
                    &endpoint.authority(host),
                    http1::FAKE_HEADERS,
                );
                Box::new(http1::SkipHead::new(endpoint.connect().await?))
            }
            Transport::Ws {
                path,
                host,
                early_data,
            } => {
                sent = head.len().min(*early_data);
                Box::new(
                    ws::connect(
                        endpoint.connect().await?,
                        &path_or_root(path),
                        &endpoint.authority(host),
                        &head[..sent],
                    )
                    .await?,
                )
            }
            Transport::HttpUpgrade { path, host } => Box::new(
                http1::upgrade(
                    endpoint.connect().await?,
                    &path_or_root(path),
                    &endpoint.authority(host),
                )
                .await?,
            ),
            Transport::Grpc {
                service_name,
                authority,
                multi,
            } => http2::grpc(&self.h2, endpoint, service_name, authority, *multi).await?,
            Transport::H2 { path, host } => {
                http2::stream(&self.h2, endpoint, &path_or_root(path), host).await?
            }
            Transport::Xhttp {
                path, host, mode, ..
            } => xhttp::open(self, path, host, *mode).await?,
        };

        preamble.extend_from_slice(&head[sent..]);
        let io = |e: io::Error| AppError::Protocol(format!("write request head: {e}"));
        if !preamble.is_empty() {
            stream.write_all(&preamble).await.map_err(io)?;
        }
        stream.flush().await.map_err(io)?;
        Ok(stream)
    }
}

/// Where the server is and how to open a connection to it: TCP, then TLS
/// when the link has it. Cloned by upload tasks that open their own.
#[derive(Clone)]
struct Endpoint {
    host: String,
    port: u16,
    tls: Option<Tls>,
}

#[derive(Clone)]
struct Tls {
    config: Arc<ClientConfig>,
    sni: String,
    /// The ALPN offered is HTTP/1.1 and nothing else.
    http1_only: bool,
}

impl Endpoint {
    async fn connect(&self) -> AppResult<Box<dyn IoStream>> {
        Ok(match &self.tls {
            Some(tls) => Box::new(
                tls::connect(&self.host, self.port, &tls.sni, Arc::clone(&tls.config)).await?,
            ),
            None => Box::new(tls::dial(&self.host, self.port).await?),
        })
    }

    /// A connection that speaks HTTP/2: over TLS only if the server agreed
    /// to it in the handshake, otherwise h2c straight away.
    async fn connect_h2(&self) -> AppResult<Box<dyn IoStream>> {
        let Some(tls) = &self.tls else {
            return Ok(Box::new(tls::dial(&self.host, self.port).await?));
        };
        let stream = tls::connect(&self.host, self.port, &tls.sni, Arc::clone(&tls.config)).await?;
        if stream.get_ref().1.alpn_protocol() != Some(b"h2") {
            return Err(AppError::Protocol(
                "the server did not agree to HTTP/2, which this transport needs".into(),
            ));
        }
        Ok(Box::new(stream))
    }

    fn scheme(&self) -> &'static str {
        if self.tls.is_some() {
            "https"
        } else {
            "http"
        }
    }

    /// The `Host` an HTTP transport sends: one of the link's hosts, else the
    /// SNI, else the server's own address.
    fn authority(&self, configured: &str) -> String {
        let host = pick(configured)
            .map(str::to_string)
            .or_else(|| self.tls.as_ref().map(|tls| tls.sni.clone()))
            .unwrap_or_else(|| self.host.clone());
        if host.parse::<std::net::Ipv6Addr>().is_ok() {
            format!("[{host}]")
        } else {
            host
        }
    }
}

/// One entry of a comma-separated list, chosen at random: links may list
/// several hosts or paths for each connection to pick from.
fn pick(list: &str) -> Option<&str> {
    let entries: Vec<&str> = list
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .collect();
    if entries.is_empty() {
        return None;
    }
    Some(entries[random_u32() as usize % entries.len()])
}

fn path_or_root(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

/// Fills `bytes` from the OS's secure random source.
fn random(bytes: &mut [u8]) {
    use ring::rand::SecureRandom;
    // Fails only if the OS has no randomness at all, which nothing here
    // could fix; the bytes are masks and padding, not keys.
    let _ = ring::rand::SystemRandom::new().fill(bytes);
}

fn random_u32() -> u32 {
    let mut bytes = [0u8; 4];
    random(&mut bytes);
    u32::from_le_bytes(bytes)
}

/// A stream whose reads and writes go different ways: XHTTP and gRPC carry
/// the two directions in separate requests, or separate halves of one.
struct Joined<R, W> {
    reader: R,
    writer: W,
}

impl<R, W> Joined<R, W> {
    fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }
}

impl<R: AsyncRead + Unpin, W: Unpin> AsyncRead for Joined<R, W> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().reader).poll_read(cx, buf)
    }
}

impl<R: Unpin, W: AsyncWrite + Unpin> AsyncWrite for Joined<R, W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().writer).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().writer).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().writer).poll_shutdown(cx)
    }
}

/// Framed bytes not yet written, for writers that wrap what they are given.
/// A write is only reported done once its whole frame is out, so nothing is
/// left sitting in here when the caller stops writing.
#[derive(Default)]
struct Outbox {
    bytes: Vec<u8>,
    sent: usize,
    /// How much of the caller's buffer the frame in flight stands for.
    pending: Option<usize>,
}

impl Outbox {
    /// Room for frames of the writer's own, sent ahead of the next write.
    fn queue(&mut self) -> &mut Vec<u8> {
        &mut self.bytes
    }

    fn poll_drain<W: AsyncWrite + Unpin>(
        &mut self,
        inner: &mut W,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<()>> {
        while self.sent < self.bytes.len() {
            let written = ready!(Pin::new(&mut *inner).poll_write(cx, &self.bytes[self.sent..]))?;
            if written == 0 {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
            }
            self.sent += written;
        }
        self.bytes.clear();
        self.sent = 0;
        Poll::Ready(Ok(()))
    }

    /// Frames `data` with `encode` and writes it out. After `Pending` the
    /// caller must come back with the same data, as `write_all` does.
    fn poll_write<W: AsyncWrite + Unpin>(
        &mut self,
        inner: &mut W,
        cx: &mut Context<'_>,
        data: &[u8],
        encode: impl FnOnce(&mut Vec<u8>, &[u8]),
    ) -> Poll<io::Result<usize>> {
        if self.pending.is_none() {
            ready!(self.poll_drain(inner, cx))?;
            encode(&mut self.bytes, data);
            self.pending = Some(data.len());
        }
        ready!(self.poll_drain(inner, cx))?;
        Poll::Ready(Ok(self.pending.take().unwrap_or(0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn each_transport_offers_the_alpn_its_framing_needs() {
        let configured = strings(&["h2", "http/1.1"]);
        let ws = Transport::Ws {
            path: "/".into(),
            host: String::new(),
            early_data: 0,
        };
        assert_eq!(alpn(&ws, &configured), strings(&["http/1.1"]));
        let grpc = Transport::Grpc {
            service_name: "svc".into(),
            authority: String::new(),
            multi: false,
        };
        assert_eq!(alpn(&grpc, &configured), strings(&["h2"]));
        assert_eq!(alpn(&Transport::Tcp, &configured), configured);

        let xhttp = Transport::Xhttp {
            path: "/".into(),
            host: String::new(),
            mode: Default::default(),
            extra: String::new(),
        };
        assert_eq!(alpn(&xhttp, &configured), strings(&["h2"]));
        assert_eq!(alpn(&xhttp, &strings(&["h3"])), strings(&["h2"]));
        assert_eq!(
            alpn(&xhttp, &strings(&["http/1.1"])),
            strings(&["http/1.1"])
        );
    }

    #[test]
    fn hosts_fall_back_from_the_link_to_the_sni_to_the_address() {
        let mut endpoint = Endpoint {
            host: "2001:db8::1".into(),
            port: 80,
            tls: None,
        };
        assert_eq!(endpoint.authority("cdn.example.com"), "cdn.example.com");
        assert_eq!(endpoint.authority(""), "[2001:db8::1]");

        endpoint.tls = Some(Tls {
            config: tls::build(
                TlsSettings {
                    sni: "aka.ms",
                    alpn: &[],
                    allow_insecure: true,
                },
                "edge.example.org",
            )
            .unwrap(),
            sni: "aka.ms".into(),
            http1_only: false,
        });
        assert_eq!(endpoint.authority(" , "), "aka.ms");
    }

    #[test]
    fn a_list_entry_is_picked_from_what_is_there() {
        assert_eq!(pick(""), None);
        assert_eq!(pick(" a.example "), Some("a.example"));
        for _ in 0..20 {
            assert!(matches!(pick("a,b, c"), Some("a" | "b" | "c")));
        }
        assert_eq!(path_or_root(""), "/");
        assert_eq!(path_or_root("ws"), "/ws");
    }
}
