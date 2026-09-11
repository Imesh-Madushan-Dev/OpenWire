//! Latency probing.
//!
//! TCP handshake time rather than ICMP: it needs no elevated privileges, and it
//! measures the path the tunnel will actually use (including any middlebox that
//! would stall the real connection).

use std::time::{Duration, Instant};

use tokio::net::TcpStream;
use tokio::time::timeout;

const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const ATTEMPTS: usize = 2;

/// Best-of-N handshake latency in milliseconds. `None` means unreachable.
pub async fn tcp_latency(host: &str, port: u16) -> Option<u32> {
    if host.is_empty() || port == 0 {
        return None;
    }

    let mut best: Option<u32> = None;
    for _ in 0..ATTEMPTS {
        let started = Instant::now();
        match timeout(PROBE_TIMEOUT, TcpStream::connect((host, port))).await {
            Ok(Ok(stream)) => {
                let elapsed = started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
                drop(stream);
                best = Some(best.map_or(elapsed, |b: u32| b.min(elapsed)));
            }
            // Timed out or refused: try again, but a single success is enough.
            _ => continue,
        }
    }
    best
}

/// Probes many nodes at once, bounded so a large catalog cannot open hundreds of
/// sockets simultaneously.
pub async fn probe_all(targets: Vec<(String, String, u16)>) -> Vec<(String, Option<u32>)> {
    use std::sync::Arc;
    use tokio::sync::Semaphore;

    let permits = Arc::new(Semaphore::new(8));
    let mut handles = Vec::with_capacity(targets.len());

    for (id, host, port) in targets {
        let permits = Arc::clone(&permits);
        handles.push(tokio::spawn(async move {
            // Semaphore is never closed, so acquire cannot fail.
            let _permit = permits.acquire_owned().await.ok();
            (id, tcp_latency(&host, port).await)
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

    #[tokio::test]
    async fn unroutable_targets_report_none_instead_of_hanging() {
        assert_eq!(tcp_latency("", 443).await, None);
        assert_eq!(tcp_latency("example.com", 0).await, None);
    }

    #[tokio::test]
    async fn measures_a_live_listener() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            // Accept whatever the probe opens so the handshake completes.
            loop {
                if listener.accept().await.is_err() {
                    break;
                }
            }
        });

        let latency = tcp_latency("127.0.0.1", port).await;
        assert!(latency.is_some(), "loopback probe should succeed");
    }
}
