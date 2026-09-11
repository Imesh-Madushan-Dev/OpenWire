//! Tunnel supervisor: owns the local listener, routes each connection, relays
//! bytes, and watches the remote's health.

pub mod inbound;
pub mod vless;

use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio::time::timeout;

use crate::core::metrics::Metrics;
use crate::core::models::{Outbound, ServerNode};
use crate::core::probe;
use crate::core::usage::{ConnRecorder, TrafficKind, UsageKey, UsageTracker};
use crate::error::{AppError, AppResult};

const RELAY_BUFFER: usize = 16 * 1024;
/// A connection with no traffic in either direction for this long is dropped,
/// so a tray app running for days cannot accumulate dead tasks.
const IDLE_TIMEOUT: Duration = Duration::from_secs(600);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const DIRECT_DIAL_TIMEOUT: Duration = Duration::from_secs(12);
const HEALTH_INTERVAL: Duration = Duration::from_secs(8);
/// Consecutive probe failures before the session is declared degraded.
const HEALTH_FAILURES: u32 = 3;

pub trait IoStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> IoStream for T {}

#[derive(Debug, Clone)]
pub enum TunnelEvent {
    Healthy,
    Degraded,
}

pub struct TunnelConfig {
    pub listen_port: u16,
    pub node: ServerNode,
    pub lan_bypass: bool,
    pub kill_switch: bool,
}

/// Everything a connection task needs, shared by `Arc` rather than cloned.
struct Session {
    node: ServerNode,
    lan_bypass: bool,
    kill_switch: bool,
    metrics: Arc<Metrics>,
    usage: Arc<UsageTracker>,
    /// Set when health checks fail. With the kill switch on, this makes the
    /// proxy refuse connections rather than let traffic find another way out.
    degraded: AtomicBool,
    tls: Option<Arc<rustls::ClientConfig>>,
}

pub struct Tunnel {
    pub local_addr: SocketAddr,
    shutdown: watch::Sender<bool>,
}

impl Tunnel {
    /// Binds the local listener and starts accepting. Returns once the socket is
    /// live, so a port conflict surfaces as a connect error, not a silent no-op.
    pub async fn start(
        config: TunnelConfig,
        metrics: Arc<Metrics>,
        usage: Arc<UsageTracker>,
        events: mpsc::UnboundedSender<TunnelEvent>,
    ) -> AppResult<Self> {
        let node = config.node;
        if !node.is_configured() {
            return Err(AppError::MissingCredentials(node.name.clone()));
        }

        // Build the TLS config once per session, not once per connection.
        let tls = match &node.outbound {
            Outbound::Vless(cfg) => Some(vless::build_tls(cfg, &node.host)?),
            Outbound::Direct => None,
        };

        // Loopback only. Binding 0.0.0.0 would turn this into an open proxy for
        // anyone on the same network.
        let listener = TcpListener::bind(("127.0.0.1", config.listen_port))
            .await
            .map_err(|source| AppError::Bind {
                port: config.listen_port,
                source,
            })?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| AppError::Internal(format!("local_addr: {e}")))?;

        let session = Arc::new(Session {
            node,
            lan_bypass: config.lan_bypass,
            kill_switch: config.kill_switch,
            metrics,
            usage,
            degraded: AtomicBool::new(false),
            tls,
        });

        let (shutdown, mut rx) = watch::channel(false);

        // Accept loop.
        {
            let session = Arc::clone(&session);
            let mut rx = rx.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = rx.changed() => break,
                        accepted = listener.accept() => match accepted {
                            Ok((stream, peer)) => {
                                // Defence in depth: the bind is loopback, but a
                                // non-local peer here means something is wrong.
                                if !peer.ip().is_loopback() {
                                    log::warn!("rejected non-loopback client {peer}");
                                    continue;
                                }
                                let session = Arc::clone(&session);
                                tokio::spawn(async move {
                                    session.metrics.open_connection();
                                    if let Err(err) = handle(stream, Arc::clone(&session)).await {
                                        log::debug!("connection closed: {err}");
                                    }
                                    session.metrics.close_connection();
                                });
                            }
                            Err(err) => {
                                log::warn!("accept failed: {err}");
                                // Back off briefly so a persistent error (fd
                                // exhaustion) cannot spin the CPU.
                                tokio::time::sleep(Duration::from_millis(200)).await;
                            }
                        },
                    }
                }
                log::info!("listener on {local_addr} stopped");
            });
        }

        // Health watcher.
        {
            let session = Arc::clone(&session);
            tokio::spawn(async move {
                let mut failures = 0u32;
                loop {
                    tokio::select! {
                        _ = rx.changed() => break,
                        _ = tokio::time::sleep(HEALTH_INTERVAL) => {}
                    }

                    // The diagnostic node has no remote to probe.
                    let latency = match session.node.outbound {
                        Outbound::Direct => Some(0),
                        Outbound::Vless(_) => {
                            probe::tcp_latency(&session.node.host, session.node.port).await
                        }
                    };

                    session.metrics.set_latency(latency);

                    if latency.is_some() {
                        failures = 0;
                        if session.degraded.swap(false, Ordering::Relaxed) {
                            let _ = events.send(TunnelEvent::Healthy);
                        }
                    } else {
                        failures += 1;
                        if failures == HEALTH_FAILURES
                            && !session.degraded.swap(true, Ordering::Relaxed)
                        {
                            let _ = events.send(TunnelEvent::Degraded);
                        }
                    }
                }
            });
        }

        log::info!(
            "tunnel up on {local_addr} via {} ({})",
            session.node.name,
            match session.node.outbound {
                Outbound::Direct => "direct",
                Outbound::Vless(_) => "vless-tls",
            }
        );

        Ok(Self {
            local_addr,
            shutdown,
        })
    }

    /// Signals every task to wind down. In-flight relays finish on their own.
    pub fn stop(&self) {
        let _ = self.shutdown.send(true);
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---------------------------------------------------------------------------
// Per-connection handling
// ---------------------------------------------------------------------------

async fn handle(mut client: TcpStream, session: Arc<Session>) -> AppResult<()> {
    let _ = client.set_nodelay(true);

    // Resolved while the socket is certainly in the OS table - a moment later
    // the process may have exited and the attribution would be lost.
    let client_port = client.peer_addr().map(|addr| addr.port()).unwrap_or(0);
    let app = session.usage.app_for_port(client_port);

    let request = timeout(HANDSHAKE_TIMEOUT, inbound::negotiate(&mut client))
        .await
        .map_err(|_| AppError::Protocol("client handshake timed out".into()))??;

    // Fail closed: while degraded with the kill switch armed, nothing gets out.
    if session.kill_switch && session.degraded.load(Ordering::Relaxed) {
        let _ = inbound::send_reply(&mut client, request.protocol, false).await;
        return Err(AppError::Protocol("blocked by kill switch".into()));
    }

    let bypass = session.lan_bypass && is_local_destination(&request.host);
    let direct = bypass || matches!(session.node.outbound, Outbound::Direct);

    let upstream: Box<dyn IoStream> = match dial(&session, &request, direct).await {
        Ok(stream) => stream,
        Err(err) => {
            let _ = inbound::send_reply(&mut client, request.protocol, false).await;
            return Err(err);
        }
    };

    inbound::send_reply(&mut client, request.protocol, true).await?;

    // Held for the lifetime of the relay: dropping it flushes the final bytes
    // and deregisters the connection.
    let usage = session.usage.open(
        UsageKey {
            app,
            host: request.host.clone(),
            kind: TrafficKind::from_port(request.port),
        },
        Arc::clone(&session.metrics),
    );

    // VLESS answers with its own 2+N byte head that the client must never see.
    let strip_head = !direct;
    relay(
        client,
        upstream,
        Arc::clone(&usage.recorder),
        request.prefix,
        strip_head,
    )
    .await
}

async fn dial(
    session: &Session,
    request: &inbound::Request,
    direct: bool,
) -> AppResult<Box<dyn IoStream>> {
    if direct {
        let stream = timeout(
            DIRECT_DIAL_TIMEOUT,
            TcpStream::connect((request.host.as_str(), request.port)),
        )
        .await
        .map_err(|_| AppError::Dial(format!("{}:{} timed out", request.host, request.port)))?
        .map_err(|e| AppError::Dial(format!("{}:{}: {e}", request.host, request.port)))?;
        let _ = stream.set_nodelay(true);
        return Ok(Box::new(stream));
    }

    let Outbound::Vless(cfg) = &session.node.outbound else {
        return Err(AppError::Internal("outbound/tls mismatch".into()));
    };
    let tls = session
        .tls
        .clone()
        .ok_or_else(|| AppError::Internal("tls config missing".into()))?;

    let stream = vless::dial(
        &session.node.host,
        session.node.port,
        cfg,
        tls,
        &request.host,
        request.port,
    )
    .await?;
    Ok(Box::new(stream))
}

async fn relay(
    client: TcpStream,
    upstream: Box<dyn IoStream>,
    recorder: Arc<ConnRecorder>,
    prefix: Vec<u8>,
    strip_head: bool,
) -> AppResult<()> {
    let (client_read, client_write) = tokio::io::split(client);
    let (upstream_read, mut upstream_write) = tokio::io::split(upstream);

    if !prefix.is_empty() {
        upstream_write
            .write_all(&prefix)
            .await
            .map_err(|e| AppError::Protocol(format!("forward request head: {e}")))?;
        recorder.add_up(prefix.len() as u64);
    }

    let up = pump(
        client_read,
        upstream_write,
        Arc::clone(&recorder),
        Direction::Up,
        false,
    );
    let down = pump(
        upstream_read,
        client_write,
        recorder,
        Direction::Down,
        strip_head,
    );

    // Both directions run to completion; each closes its write half on EOF so
    // the peer learns the stream is finished.
    let (up_result, down_result) = tokio::join!(up, down);
    up_result.or(down_result)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Up,
    Down,
}

async fn pump<R, W>(
    mut reader: R,
    mut writer: W,
    recorder: Arc<ConnRecorder>,
    direction: Direction,
    strip_head: bool,
) -> AppResult<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let io = |e: std::io::Error| AppError::Protocol(format!("relay: {e}"));

    // Running in its own task means this blocking read cannot stall the other
    // direction - which is what lets client-speaks-first protocols work.
    if strip_head {
        let mut head = [0u8; vless::RESPONSE_HEAD_MIN];
        timeout(IDLE_TIMEOUT, reader.read_exact(&mut head))
            .await
            .map_err(|_| AppError::Protocol("remote never answered".into()))?
            .map_err(io)?;
        if head[1] > 0 {
            let mut addons = vec![0u8; head[1] as usize];
            timeout(IDLE_TIMEOUT, reader.read_exact(&mut addons))
                .await
                .map_err(|_| AppError::Protocol("remote head truncated".into()))?
                .map_err(io)?;
        }
    }

    let mut buf = vec![0u8; RELAY_BUFFER];
    loop {
        let read = timeout(IDLE_TIMEOUT, reader.read(&mut buf))
            .await
            .map_err(|_| AppError::Protocol("connection idle".into()))?
            .map_err(io)?;
        if read == 0 {
            break;
        }
        writer.write_all(&buf[..read]).await.map_err(io)?;
        match direction {
            Direction::Up => recorder.add_up(read as u64),
            Direction::Down => recorder.add_down(read as u64),
        }
    }

    let _ = writer.shutdown().await;
    Ok(())
}

/// Destinations that stay off the tunnel when LAN bypass is on.
fn is_local_destination(host: &str) -> bool {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => {
                v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_broadcast()
            }
            IpAddr::V6(v6) => {
                // `is_unique_local`/`is_unicast_link_local` are still unstable,
                // so match the fc00::/7 and fe80::/10 prefixes directly.
                v6.is_loopback()
                    || (v6.segments()[0] & 0xfe00) == 0xfc00
                    || (v6.segments()[0] & 0xffc0) == 0xfe80
            }
        };
    }

    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || !host.contains('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lan_bypass_matches_private_space_only() {
        for local in [
            "127.0.0.1",
            "10.1.2.3",
            "192.168.0.5",
            "172.16.0.1",
            "169.254.1.1",
            "::1",
            "fd00::1",
            "fe80::1",
            "localhost",
            "printer.local",
            "router",
            "db.internal",
        ] {
            assert!(is_local_destination(local), "{local} should bypass");
        }

        for remote in [
            "203.0.113.10",
            "8.8.8.8",
            "example.com",
            "teams.microsoft.com",
            "2606:4700::1111",
            // 172.32 is outside the private 172.16/12 block.
            "172.32.0.1",
        ] {
            assert!(!is_local_destination(remote), "{remote} should tunnel");
        }
    }

    /// End-to-end through the real relay: a client speaks SOCKS5 to the tunnel,
    /// the tunnel dials a local echo server, and bytes come back counted.
    #[tokio::test]
    async fn direct_mode_relays_and_counts_traffic() {
        let echo = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_port = echo.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut stream, _) = echo.accept().await.unwrap();
            let (mut r, mut w) = stream.split();
            let _ = tokio::io::copy(&mut r, &mut w).await;
        });

        let metrics = Arc::new(Metrics::new());
        let usage = Arc::new(UsageTracker::load(std::path::Path::new("no-such-usage.json")));
        let (tx, _rx) = mpsc::unbounded_channel();
        let tunnel = Tunnel::start(
            TunnelConfig {
                // Port 0 lets the OS pick, so the test never collides.
                listen_port: 0,
                node: crate::core::catalog::default_nodes()
                    .into_iter()
                    .find(|n| n.id == "maizy-direct")
                    .unwrap(),
                lan_bypass: false,
                kill_switch: false,
            },
            Arc::clone(&metrics),
            Arc::clone(&usage),
            tx,
        )
        .await
        .expect("tunnel should start");

        let mut client = TcpStream::connect(tunnel.local_addr).await.unwrap();
        client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut ack = [0u8; 2];
        client.read_exact(&mut ack).await.unwrap();

        let mut req = vec![0x05, 0x01, 0x00, 0x01, 127, 0, 0, 1];
        req.extend_from_slice(&echo_port.to_be_bytes());
        client.write_all(&req).await.unwrap();

        let mut reply = [0u8; 10];
        client.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[1], 0x00, "socks reply should report success");

        client.write_all(b"maizy").await.unwrap();
        let mut echoed = [0u8; 5];
        client.read_exact(&mut echoed).await.unwrap();
        assert_eq!(&echoed, b"maizy");

        let snap = metrics.snapshot();
        assert_eq!(snap.up_bytes, 5);
        assert_eq!(snap.down_bytes, 5);
        assert_eq!(snap.total_connections, 1);

        // The same bytes must also arrive attributed. This covers the whole
        // chain: accept -> resolve the owning process -> key the connection ->
        // fold into the day bucket.
        usage.sample();
        let summary = usage.summary(crate::core::usage::history::Range::Day);
        assert_eq!(summary.tunnel_up, 5);
        assert_eq!(summary.tunnel_down, 5);

        let app = &summary.apps[0].key;
        assert!(
            app.ends_with(".exe"),
            "expected the calling process to be named, got {app}"
        );
        assert_eq!(summary.hosts[0].key, "127.0.0.1");
        // The echo server is on an ephemeral port, so it classifies as Other.
        assert_eq!(summary.kinds[0].key, "Other");

        tunnel.stop();
    }

    #[tokio::test]
    async fn unconfigured_node_is_refused_before_binding() {
        let metrics = Arc::new(Metrics::new());
        let usage = Arc::new(UsageTracker::load(std::path::Path::new("no-such-usage.json")));
        let (tx, _rx) = mpsc::unbounded_channel();
        let node = crate::core::catalog::default_nodes()
            .into_iter()
            .find(|n| n.id == "sg-01")
            .unwrap();

        let result = Tunnel::start(
            TunnelConfig {
                listen_port: 0,
                node,
                lan_bypass: true,
                kill_switch: false,
            },
            metrics,
            usage,
            tx,
        )
        .await;

        match result {
            Err(err) => assert_eq!(err.code(), "MISSING_CREDENTIALS"),
            Ok(_) => panic!("an unconfigured node must never bind a listener"),
        }
    }
}
