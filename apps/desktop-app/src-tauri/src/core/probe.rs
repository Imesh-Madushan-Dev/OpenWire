//! Latency probing.
//!
//! One round trip to the server: from the tunnel's own ClientHello leaving to
//! the first byte of the answer. Not ICMP, which needs elevated privileges.
//! And not the TCP handshake alone, which is what this used to time: a VPN
//! with a TUN or TAP adapter completes the handshake on this machine and only
//! then dials out, so every server read 0 ms. A ClientHello has to reach
//! whatever really answers.
//!
//! Every server Maizy speaks to is behind TLS, so a ClientHello is a message
//! it answers - and it is sent with the same SNI and ALPN as the real
//! connection, so a network that treats the two differently cannot make the
//! probe lie about the tunnel. Nothing past the first byte back is read: no
//! certificate is checked and no credentials are sent.

use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::core::models::ServerNode;
use crate::core::tunnel::tls;

const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const ATTEMPTS: usize = 2;

/// A server, as the probe needs to reach it: the way the tunnel does.
#[derive(Debug, Clone)]
pub struct Target {
    pub host: String,
    pub port: u16,
    /// The name the tunnel's ClientHello carries; the host when it has none.
    pub sni: String,
    pub alpn: Vec<String>,
}

impl Target {
    pub fn of(node: &ServerNode) -> Self {
        let tls = node.outbound.tls();
        Self {
            host: node.host.clone(),
            port: node.port,
            sni: tls
                .as_ref()
                .map(|tls| tls.sni)
                .filter(|sni| !sni.is_empty())
                .unwrap_or(node.host.as_str())
                .to_string(),
            alpn: tls.map(|tls| tls.alpn.to_vec()).unwrap_or_default(),
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
    let hello = tls::client_hello(&target.sni, &target.alpn)?;

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
        // dial failed hangs up at once, from this machine.
        if let Ok(Ok(1..)) = timeout(wait, stream.read(&mut first)).await {
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
            sni: "example.com".to_string(),
            alpn: vec!["h2".to_string()],
        }
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

