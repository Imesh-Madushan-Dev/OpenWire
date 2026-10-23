//! Latency probing.
//!
//! One round trip to the server: from the tunnel's own ClientHello leaving to
//! the first byte of the answer. Not ICMP, which needs elevated privileges.
//! And not the TCP handshake alone, which is what this used to time: a VPN
//! with a TUN or TAP adapter completes the handshake on this machine and only
//! then dials out, so every server read 0 ms. A ClientHello has to reach
//! whatever really answers.
//!
//! A server behind TLS answers a ClientHello - and it is sent with the same
//! SNI and ALPN as the real connection, so a network that treats the two
//! differently cannot make the probe lie about the tunnel. A server without
//! TLS gets what its transport would say first instead. Nothing past the
//! first byte back is read: no certificate is checked and no credentials
//! are sent.

use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::core::models::{ServerNode, Transport};
use crate::core::tunnel::{tls, transport};

const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const ATTEMPTS: usize = 2;

/// HTTP/2's connection preface, then an empty SETTINGS frame.
const H2_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n\x00\x00\x00\x04\x00\x00\x00\x00\x00";

/// A server, as the probe needs to reach it: the way the tunnel does.
#[derive(Debug, Clone)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub greeting: Greeting,
}

/// What the probe opens with: something the server answers before any
/// credentials, said the way the tunnel would say it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Greeting {
    /// A ClientHello. `sni` is the host when the server has none.
    Tls { sni: String, alpn: Vec<String> },
    /// The HTTP/2 preface, for a gRPC or h2 server without TLS.
    H2c,
    /// An upgrade request to the transport's own path, for the other HTTP
    /// transports without TLS. An HTTPUpgrade server answers nothing else.
    Http { host: String, path: String },
    /// Raw TCP without TLS, fake HTTP header or not. A server like that says
    /// nothing to a stranger, so it hanging up is the only answer there is.
    Raw { host: String },
}

impl Greeting {
    fn bytes(&self) -> Option<Vec<u8>> {
        match self {
            Self::Tls { sni, alpn } => tls::client_hello(sni, alpn),
            Self::H2c => Some(H2_PREFACE.to_vec()),
            Self::Http { host, path } => Some(
                format!(
                    "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n"
                )
                .into_bytes(),
            ),
            // Twice: a server behind a fake HTTP header takes the first as
            // the disguise and gives up on the second, as a raw one gives up
            // on the first.
            Self::Raw { host } => {
                Some(format!("GET / HTTP/1.1\r\nHost: {host}\r\n\r\n").repeat(2).into_bytes())
            }
        }
    }
}

impl Target {
    pub fn of(node: &ServerNode) -> Self {
        let transport = node.outbound.transport().cloned().unwrap_or_default();
        let greeting = match node.outbound.tls() {
            Some(tls) => Greeting::Tls {
                sni: if tls.sni.is_empty() {
                    node.host.clone()
                } else {
                    tls.sni.to_string()
                },
                alpn: transport::alpn(&transport, tls.alpn),
            },
            None => match &transport {
                Transport::Tcp | Transport::TcpHttp { .. } => Greeting::Raw {
                    host: node.host.clone(),
                },
                Transport::Grpc { .. } | Transport::H2 { .. } => Greeting::H2c,
                Transport::Ws { host, path, .. }
                | Transport::HttpUpgrade { host, path }
                | Transport::Xhttp { host, path, .. } => {
                    let first = |list: &str| {
                        list.split(',')
                            .map(str::trim)
                            .find(|entry| !entry.is_empty())
                            .map(str::to_string)
                    };
                    let path = first(path).unwrap_or_default();
                    Greeting::Http {
                        host: first(host).unwrap_or_else(|| node.host.clone()),
                        path: if path.starts_with('/') {
                            path
                        } else {
                            format!("/{path}")
                        },
                    }
                }
            },
        };
        Self {
            host: node.host.clone(),
            port: node.port,
            greeting,
        }
    }
}

/// Best-of-N round trip in milliseconds. `None` means nothing answered.
pub async fn latency(target: &Target) -> Option<u32> {
    round_trip(target, PROBE_TIMEOUT).await
}

async fn round_trip(target: &Target, wait: Duration) -> Option<u32> {
    if target.host.is_empty() || target.port == 0 {
        return None;
    }
    let hello = target.greeting.bytes()?;
    let hangup_answers = matches!(target.greeting, Greeting::Raw { .. });

    let mut best: Option<u32> = None;
    for _ in 0..ATTEMPTS {
        let Ok(Ok(mut stream)) =
            timeout(wait, TcpStream::connect((target.host.as_str(), target.port))).await
        else {
            // Timed out or refused: try again, but a single success is enough.
            continue;
        };
        let _ = stream.set_nodelay(true);

        let started = Instant::now();
        if stream.write_all(&hello).await.is_err() {
            continue;
        }
        let mut first = [0u8; 1];
        // A byte, not just the end of the stream: an interceptor whose own
        // dial failed hangs up at once, from this machine. Only a raw server
        // without TLS has nothing else to give.
        let answered = match timeout(wait, stream.read(&mut first)).await {
            Ok(Ok(1..)) => true,
            Ok(Ok(0)) => hangup_answers,
            _ => false,
        };
        if answered {
            let elapsed = started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
            best = Some(best.map_or(elapsed, |b: u32| b.min(elapsed)));
        }
    }
    best
}

/// Probes many nodes at once, bounded so a large catalog cannot open hundreds of
/// sockets simultaneously.
pub async fn probe_all(targets: Vec<(String, Target)>) -> Vec<(String, Option<u32>)> {
    use std::sync::Arc;
    use tokio::sync::Semaphore;

    let permits = Arc::new(Semaphore::new(8));
    let mut handles = Vec::with_capacity(targets.len());

    for (id, target) in targets {
        let permits = Arc::clone(&permits);
        handles.push(tokio::spawn(async move {
            // Semaphore is never closed, so acquire cannot fail.
            let _permit = permits.acquire_owned().await.ok();
            (id, latency(&target).await)
        }));
    }

    let mut out = Vec::with_capacity(handles.len());
    for handle in handles {
        if let Ok(result) = handle.await {
            out.push(result);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(host: &str, port: u16) -> Target {
        Target {
            host: host.to_string(),
            port,
            greeting: Greeting::Tls {
                sni: "example.com".to_string(),
                alpn: vec!["h2".to_string()],
            },
        }
    }

    #[test]
    fn each_server_is_greeted_the_way_its_tunnel_opens() {
        let tls = crate::core::catalog::parse_share_link(
            "vless://k@edge.example.org:443?security=tls&sni=aka.ms&type=ws&path=%2Fws",
        )
        .unwrap();
        assert_eq!(
            Target::of(&tls).greeting,
            Greeting::Tls {
                sni: "aka.ms".into(),
                alpn: vec!["http/1.1".into()]
            }
        );

        let plain_ws = crate::core::catalog::parse_share_link(
            "vless://k@edge.example.org:80?security=none&type=ws&path=%2Fws",
        )
        .unwrap();
        assert!(matches!(Target::of(&plain_ws).greeting, Greeting::Http { .. }));

        let plain_grpc = crate::core::catalog::parse_share_link(
            "trojan://p@edge.example.org:80?security=none&type=grpc&serviceName=svc",
        )
        .unwrap();
        assert_eq!(Target::of(&plain_grpc).greeting, Greeting::H2c);
    }

    #[tokio::test]
    async fn unroutable_targets_report_none_instead_of_hanging() {
        assert_eq!(latency(&target("", 443)).await, None);
        assert_eq!(latency(&target("example.com", 0)).await, None);
    }

    #[test]
    fn the_hello_is_a_tls_handshake_record() {
        let hello = tls::client_hello("example.com", &[]).expect("a ClientHello");
        // Content type 22 (handshake), then message type 1 (ClientHello).
        assert_eq!(hello[0], 22);
        assert_eq!(hello[5], 1);
    }

    #[tokio::test]
    async fn times_a_server_that_answers_the_hello() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buffer = [0u8; 512];
                    if socket.read(&mut buffer).await.unwrap_or(0) > 0 {
                        // Any reply will do; a real server sends a ServerHello.
                        let _ = socket.write_all(&[21, 3, 3, 0, 2, 2, 40]).await;
                    }
                });
            }
        });

        assert!(latency(&target("127.0.0.1", port)).await.is_some());
    }

    /// What a local TUN/TAP VPN looks like: the handshake completes at once,
    /// then nothing comes back. That must not read as an instant server.
    #[tokio::test]
    async fn a_completed_handshake_alone_is_not_an_answer() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((socket, _)) = listener.accept().await {
                held.push(socket);
            }
        });

        let answer = round_trip(&target("127.0.0.1", port), Duration::from_millis(200)).await;
        assert_eq!(answer, None);
    }
}

