//! Packets in, flows out: the userspace TCP/IP stack between the adapter and
//! the app's local proxy.
//!
//! The stack (smoltcp, through `netstack-smoltcp`) terminates every TCP
//! connection an app opens through the adapter and hands back a stream; UDP
//! arrives as datagrams. Each becomes a flow on the proxy, as [`super::wire`]
//! describes, and the app takes it from there exactly as it would a SOCKS5
//! connection.
//!
//! Nothing here knows about Windows: packets come in on a channel and go out
//! through [`PacketWriter`], so the same engine runs against the adapter in
//! the service and against hand-built packets in the tests below.
//!
//! Three kinds of traffic are treated specially:
//!
//! - DNS. Every query to port 53, over UDP or TCP and to whatever address,
//!   is answered through the tunnel - see [`super::dns`]. Nothing an app
//!   resolves leaves outside it, even if it ignores the system resolver.
//! - QUIC (UDP to port 443) is dropped. Carried over the tunnel's TCP it is
//!   slower than the TCP it replaces, and browsers fall back to TCP within a
//!   fraction of a second - the same choice v2ray and sing-box setups make.
//! - The adapter's own subnet, apart from the DNS address: Windows
//!   broadcasts NetBIOS there, and it has nowhere to go.

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::{SinkExt, StreamExt};
use netstack_smoltcp::StackBuilder;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, watch, Semaphore};
use tokio::task::JoinSet;
use tokio::time::timeout;

use super::cidr::Cidr;
use super::dns::{self, Forwarder};
use super::upstream;
use super::wire::{self, FlowRequest, Network};

/// Bytes each side of a TCP connection buffers. The window only has to span
/// the hop between Windows and this stack, which never leaves the machine,
/// so it can be far smaller than a real network's - and it is allocated for
/// every connection.
const TCP_RECEIVE_WINDOW: u32 = 32 * 1024;
const TCP_SEND_WINDOW: u32 = 64 * 1024;
/// A connection with nothing moving either way for this long is dropped, as
/// the app's own relay does.
const TCP_IDLE: Duration = Duration::from_secs(600);
/// UDP has no close; a flow quiet this long in both directions is over.
const UDP_IDLE: Duration = Duration::from_secs(60);
/// Bounds on concurrent flows, so a misbehaving app cannot exhaust sockets.
const MAX_TCP_FLOWS: usize = 4096;
const MAX_UDP_FLOWS: usize = 2048;
/// Datagrams queued for a UDP flow while its upstream leg opens.
const UDP_QUEUE: usize = 64;
const RELAY_BUFFER: usize = 16 * 1024;

/// Where the engine sends packets for the adapter.
pub trait PacketWriter: Send + Sync + 'static {
    /// Best effort: a full ring drops the packet, and TCP resends it.
    fn write(&self, packet: &[u8]);
}

#[derive(Debug, Clone)]
pub struct Config {
    /// The app's local proxy.
    pub proxy: SocketAddr,
    /// Resolvers reached through the tunnel.
    pub resolvers: Vec<SocketAddr>,
    /// Where Windows sends DNS queries; also what DNS flows are attributed to.
    pub dns_address: SocketAddr,
    /// The adapter's own subnets. Nothing but the DNS address in them is real.
    pub local_subnets: Vec<Cidr>,
    pub mtu: u16,
}

/// Shared by every flow task.
struct Context {
    proxy: SocketAddr,
    resolver: SocketAddr,
    dns: Forwarder,
    mtu: u16,
}

/// Runs until `stop` changes, or until the packet source closes.
pub async fn run(
    config: Config,
    mut packets: mpsc::Receiver<Vec<u8>>,
    writer: Arc<dyn PacketWriter>,
    mut stop: watch::Receiver<bool>,
) -> io::Result<()> {
    let resolver = *config
        .resolvers
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no resolver"))?;

    let dns_ip = config.dns_address.ip();
    let local_subnets = config.local_subnets.clone();
    let (stack, runner, udp, tcp) = StackBuilder::default()
        .enable_tcp(true)
        .enable_udp(true)
        // ICMP would have smoltcp answer every ping itself, reporting any
        // host as reachable in no time. Better to say nothing.
        .enable_icmp(false)
        .mtu(config.mtu as usize)
        .tcp_recv_buffer_size(TCP_RECEIVE_WINDOW)
        .tcp_send_buffer_size(TCP_SEND_WINDOW)
        .add_ip_filter_fn(move |_source, destination| {
            *destination == dns_ip
                || !local_subnets
                    .iter()
                    .any(|subnet| subnet.contains(&Cidr::host(*destination)))
        })
        .build()?;
    let missing = || io::Error::other("network stack built without a part it was asked for");
    let runner = runner.ok_or_else(missing)?;
    let udp = udp.ok_or_else(missing)?;
    let tcp = tcp.ok_or_else(missing)?;

    let context = Arc::new(Context {
        proxy: config.proxy,
        resolver,
        dns: Forwarder::spawn(config.proxy, config.dns_address, config.resolvers.clone()),
        mtu: config.mtu,
    });

    let (mut into_stack, mut out_of_stack) = stack.split();
    let mut tasks: JoinSet<io::Result<()>> = JoinSet::new();

    tasks.spawn(runner);
    tasks.spawn(async move {
        while let Some(packet) = packets.recv().await {
            match into_stack.send(packet).await {
                Ok(()) => {}
                // A malformed packet is dropped; the stack carries on.
                Err(err) if err.kind() == io::ErrorKind::InvalidInput => {}
                Err(err) => return Err(err),
            }
        }
        Ok(())
    });
    tasks.spawn(async move {
        while let Some(packet) = out_of_stack.next().await {
            writer.write(&packet?);
        }
        Ok(())
    });
    tasks.spawn(accept_tcp(tcp, Arc::clone(&context)));
    tasks.spawn(serve_udp(udp, context));

    let outcome = tokio::select! {
        _ = stop.changed() => Ok(()),
        Some(ended) = tasks.join_next() => match ended {
            Ok(Ok(())) => Ok(()),
            Ok(Err(err)) => Err(err),
            Err(err) => Err(io::Error::other(format!("engine task failed: {err}"))),
        },
    };
    // Dropping the set aborts every task, and every flow they own.
    tasks.shutdown().await;
    outcome
}

// ---------------------------------------------------------------------------
// TCP
// ---------------------------------------------------------------------------

async fn accept_tcp(
    mut listener: netstack_smoltcp::TcpListener,
    context: Arc<Context>,
) -> io::Result<()> {
    let limit = Arc::new(Semaphore::new(MAX_TCP_FLOWS));
    let mut flows = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.next() => {
                let Some((stream, source, destination)) = accepted else { break };
                let Ok(permit) = Arc::clone(&limit).try_acquire_owned() else {
                    log::warn!("too many connections; dropping one to {destination}");
                    continue;
                };
                let context = Arc::clone(&context);
                flows.spawn(async move {
                    tcp_flow(stream, source, destination, &context).await;
                    drop(permit);
                });
            }
            // Reaped as they finish, so the set does not grow for the life
            // of the session.
            Some(_) = flows.join_next(), if !flows.is_empty() => {}
        }
    }
    Ok(())
}

async fn tcp_flow(
    stream: netstack_smoltcp::TcpStream,
    source: SocketAddr,
    destination: SocketAddr,
    context: &Context,
) {
    // DNS over TCP, to whatever address, goes to the resolver too.
    let target = if destination.port() == 53 {
        context.resolver
    } else {
        destination
    };
    let request = FlowRequest {
        network: Network::Tcp,
        source,
        destination: target,
    };
    match upstream::open(context.proxy, request).await {
        Ok(upstream) => relay(stream, upstream).await,
        // Dropping the stream closes the app's connection.
        Err(err) => log::debug!("{source} -> {destination}: {err}"),
    }
}

/// Copies both ways until both are done or nothing moves for [`TCP_IDLE`].
async fn relay<A, B>(a: A, b: B)
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    let (a_read, a_write) = tokio::io::split(a);
    let (b_read, b_write) = tokio::io::split(b);
    let _ = tokio::join!(pump(a_read, b_write), pump(b_read, a_write));
}

async fn pump<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    mut reader: R,
    mut writer: W,
) -> io::Result<()> {
    let mut buffer = vec![0u8; RELAY_BUFFER];
    loop {
        let read = match timeout(TCP_IDLE, reader.read(&mut buffer)).await {
            Ok(Ok(0)) | Err(_) => break,
            Ok(Ok(read)) => read,
            Ok(Err(err)) => return Err(err),
        };
        writer.write_all(&buffer[..read]).await?;
        writer.flush().await?;
    }
    // Half-close, so the other side learns this direction is finished.
    let _ = writer.shutdown().await;
    Ok(())
}

// ---------------------------------------------------------------------------
// UDP
// ---------------------------------------------------------------------------

type FlowKey = (SocketAddr, SocketAddr);
/// A datagram for the adapter: payload, from, to.
type Reply = (Vec<u8>, SocketAddr, SocketAddr);

async fn serve_udp(socket: netstack_smoltcp::UdpSocket, context: Arc<Context>) -> io::Result<()> {
    let (mut datagrams, mut out) = socket.split();

    // One writer: the stack's sink is not shared, and every flow replies.
    let (replies, mut queued) = mpsc::channel::<Reply>(1024);
    let mut tasks = JoinSet::new();
    tasks.spawn(async move {
        while let Some(reply) = queued.recv().await {
            match out.send(reply).await {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::InvalidData => {}
                Err(_) => break,
            }
        }
    });

    let mut flows: HashMap<FlowKey, mpsc::Sender<Vec<u8>>> = HashMap::new();
    let (ended, mut finished) = mpsc::unbounded_channel::<FlowKey>();

    loop {
        tokio::select! {
            datagram = datagrams.next() => {
                let Some((payload, source, destination)) = datagram else { break };

                if destination.port() == 53 {
                    let context = Arc::clone(&context);
                    let replies = replies.clone();
                    tasks.spawn(async move {
                        let query = payload;
                        if let Some(answer) = context.dns.resolve(query.clone()).await {
                            let answer = dns::fit_for_udp(&query, answer, udp_ceiling(context.mtu, destination.ip()));
                            let _ = replies.send((answer, destination, source)).await;
                        }
                    });
                    continue;
                }
                if destination.port() == 443 {
                    continue; // QUIC; see the module notes
                }

                let key = (source, destination);
                if let Some(flow) = flows.get(&key) {
                    if !flow.is_closed() {
                        // A full queue drops the datagram, as a busy link would.
                        let _ = flow.try_send(payload);
                        continue;
                    }
                    flows.remove(&key);
                }
                if flows.len() >= MAX_UDP_FLOWS {
                    continue;
                }
                let (queue, inbox) = mpsc::channel(UDP_QUEUE);
                let _ = queue.try_send(payload);
                flows.insert(key, queue);
                let context = Arc::clone(&context);
                let replies = replies.clone();
                let ended = ended.clone();
                tasks.spawn(async move {
                    udp_flow(key, inbox, replies, &context).await;
                    let _ = ended.send(key);
                });
            }
            Some(key) = finished.recv() => {
                // A newer flow may already have taken the key.
                if flows.get(&key).is_some_and(|flow| flow.is_closed()) {
                    flows.remove(&key);
                }
            }
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {}
        }
    }
    Ok(())
}

/// The largest UDP payload that fits one packet on the adapter.
fn udp_ceiling(mtu: u16, destination: IpAddr) -> usize {
    let ip_header = if destination.is_ipv4() { 20 } else { 40 };
    (mtu as usize).saturating_sub(ip_header + 8)
}

async fn udp_flow(
    (source, destination): FlowKey,
    mut inbox: mpsc::Receiver<Vec<u8>>,
    replies: mpsc::Sender<Reply>,
    context: &Context,
) {
    let request = FlowRequest {
        network: Network::Udp,
        source,
        destination,
    };
    let stream = match upstream::open(context.proxy, request).await {
        Ok(stream) => stream,
        Err(err) => {
            log::debug!("udp {source} -> {destination}: {err}");
            return;
        }
    };
    let (mut reader, mut writer) = stream.into_split();

    let started = Instant::now();
    let last_active = AtomicU64::new(0);
    let touch = || last_active.store(started.elapsed().as_millis() as u64, Ordering::Relaxed);

    let up = async {
        while let Some(datagram) = inbox.recv().await {
            if wire::write_datagram(&mut writer, &datagram).await.is_err() {
                break;
            }
            touch();
        }
    };
    let down = async {
        let mut buffer = Vec::new();
        while let Ok(true) = wire::read_datagram(&mut reader, &mut buffer).await {
            touch();
            if replies
                .send((std::mem::take(&mut buffer), destination, source))
                .await
                .is_err()
            {
                break;
            }
        }
    };
    let idle = async {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let quiet = started.elapsed().as_millis() as u64 - last_active.load(Ordering::Relaxed);
            if quiet >= UDP_IDLE.as_millis() as u64 {
                break;
            }
        }
    };

    // Either direction failing, or both going quiet, ends the flow.
    tokio::select! {
        _ = up => {}
        _ = down => {}
        _ = idle => {}
    }
}

#[cfg(test)]
mod tests {
    //! Hand-built packets through the real stack, with a fake proxy behind
    //! it - the whole service data path, without an adapter or admin rights.

    use super::*;
    use etherparse::{NetSlice, PacketBuilder, SlicedPacket, TransportSlice};
    use std::sync::Mutex;
    use tokio::net::TcpListener;

    const APP: &str = "172.19.0.1";

    /// Collects what the engine writes to the "adapter".
    struct Capture(Mutex<mpsc::UnboundedSender<Vec<u8>>>);

    impl PacketWriter for Capture {
        fn write(&self, packet: &[u8]) {
            let _ = self.0.lock().unwrap().send(packet.to_vec());
        }
    }

    /// A proxy that accepts the private method, records each request, and
    /// echoes whatever the flow carries.
    async fn echo_proxy() -> (SocketAddr, mpsc::UnboundedReceiver<FlowRequest>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (seen, requests) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let seen = seen.clone();
                tokio::spawn(async move {
                    let mut greeting = [0u8; 3];
                    stream.read_exact(&mut greeting).await.unwrap();
                    let request = FlowRequest::read(&mut stream).await.unwrap();
                    let _ = seen.send(request);
                    stream.write_all(&[0x05, wire::METHOD]).await.unwrap();
                    stream
                        .write_all(&wire::status_reply(wire::Status::Ok))
                        .await
                        .unwrap();
                    let (mut r, mut w) = stream.split();
                    let _ = tokio::io::copy(&mut r, &mut w).await;
                });
            }
        });
        (address, requests)
    }

    struct Harness {
        packets: mpsc::Sender<Vec<u8>>,
        written: mpsc::UnboundedReceiver<Vec<u8>>,
        requests: mpsc::UnboundedReceiver<FlowRequest>,
        _stop: watch::Sender<bool>,
    }

    async fn harness() -> Harness {
        let (proxy, requests) = echo_proxy().await;
        let (packets, inbox) = mpsc::channel(64);
        let (capture, written) = mpsc::unbounded_channel();
        let (stop, stopped) = watch::channel(false);
        let config = Config {
            proxy,
            resolvers: vec!["1.1.1.1:53".parse().unwrap()],
            dns_address: "172.19.0.2:53".parse().unwrap(),
            local_subnets: vec![Cidr::new("172.19.0.0".parse().unwrap(), 30)],
            mtu: 1500,
        };
        tokio::spawn(run(
            config,
            inbox,
            Arc::new(Capture(Mutex::new(capture))),
            stopped,
        ));
        Harness {
            packets,
            written,
            requests,
            _stop: stop,
        }
    }

    fn ip(text: &str) -> [u8; 4] {
        text.parse::<std::net::Ipv4Addr>().unwrap().octets()
    }

    fn udp(source_port: u16, to: &str, port: u16, payload: &[u8]) -> Vec<u8> {
        let builder = PacketBuilder::ipv4(ip(APP), ip(to), 64).udp(source_port, port);
        let mut packet = Vec::with_capacity(builder.size(payload.len()));
        builder.write(&mut packet, payload).unwrap();
        packet
    }

    fn tcp(
        source_port: u16,
        to: &str,
        port: u16,
        seq: u32,
        ack: Option<u32>,
        syn: bool,
        payload: &[u8],
    ) -> Vec<u8> {
        let mut builder =
            PacketBuilder::ipv4(ip(APP), ip(to), 64).tcp(source_port, port, seq, 65535);
        if syn {
            builder = builder.syn();
        }
        if let Some(ack) = ack {
            builder = builder.ack(ack);
        }
        if !payload.is_empty() {
            builder = builder.psh();
        }
        let mut packet = Vec::with_capacity(builder.size(payload.len()));
        builder.write(&mut packet, payload).unwrap();
        packet
    }

    async fn next_written(harness: &mut Harness) -> Vec<u8> {
        timeout(Duration::from_secs(5), harness.written.recv())
            .await
            .expect("the engine answered")
            .expect("engine alive")
    }

    #[tokio::test]
    async fn a_udp_datagram_becomes_a_flow_and_its_reply_comes_back() {
        let mut harness = harness().await;
        harness
            .packets
            .send(udp(40000, "203.0.113.7", 9000, b"ping"))
            .await
            .unwrap();

        let request = timeout(Duration::from_secs(5), harness.requests.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(request.network, Network::Udp);
        assert_eq!(request.source, format!("{APP}:40000").parse().unwrap());
        assert_eq!(request.destination, "203.0.113.7:9000".parse().unwrap());

        // The proxy echoes the frame; the engine turns it back into a packet
        // from the destination to the app.
        let reply = next_written(&mut harness).await;
        let parsed = SlicedPacket::from_ip(&reply).unwrap();
        let Some(NetSlice::Ipv4(net)) = parsed.net else {
            panic!("ipv4")
        };
        assert_eq!(net.header().source(), ip("203.0.113.7"));
        assert_eq!(net.header().destination(), ip(APP));
        let Some(TransportSlice::Udp(udp)) = parsed.transport else {
            panic!("udp")
        };
        assert_eq!((udp.source_port(), udp.destination_port()), (9000, 40000));
        assert_eq!(udp.payload(), b"ping");
    }

    #[tokio::test]
    async fn a_dns_query_is_answered_through_the_resolver_from_the_address_asked() {
        let mut harness = harness().await;
        // A header-sized query is enough: the echo proxy answers with it.
        let query = [0x12, 0x34, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        harness
            .packets
            .send(udp(53000, "172.19.0.2", 53, &query))
            .await
            .unwrap();

        let request = timeout(Duration::from_secs(5), harness.requests.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            request.network,
            Network::Tcp,
            "DNS goes to the resolver over TCP"
        );
        assert_eq!(request.destination, "1.1.1.1:53".parse().unwrap());
        assert_eq!(request.source, "172.19.0.2:53".parse().unwrap());

        let reply = next_written(&mut harness).await;
        let parsed = SlicedPacket::from_ip(&reply).unwrap();
        let Some(NetSlice::Ipv4(net)) = parsed.net else {
            panic!("ipv4")
        };
        assert_eq!(net.header().source(), ip("172.19.0.2"));
        let Some(TransportSlice::Udp(udp)) = parsed.transport else {
            panic!("udp")
        };
        assert_eq!((udp.source_port(), udp.destination_port()), (53, 53000));
        assert_eq!(
            udp.payload(),
            &query,
            "the original ID is back on the answer"
        );
    }

    #[tokio::test]
    async fn quic_and_the_adapter_subnet_go_nowhere() {
        let mut harness = harness().await;
        harness
            .packets
            .send(udp(40001, "203.0.113.7", 443, b"quic"))
            .await
            .unwrap();
        harness
            .packets
            .send(udp(137, "172.19.0.3", 137, b"netbios"))
            .await
            .unwrap();
        // Then something that does get through, to know the others were
        // processed and dropped rather than still in flight.
        harness
            .packets
            .send(udp(40002, "203.0.113.7", 9001, b"real"))
            .await
            .unwrap();

        let request = timeout(Duration::from_secs(5), harness.requests.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(request.destination.port(), 9001);
        let _ = next_written(&mut harness).await;
        assert!(
            harness.requests.try_recv().is_err(),
            "nothing else reached the proxy"
        );
    }

    #[tokio::test]
    async fn a_tcp_connection_is_terminated_here_and_relayed() {
        let mut harness = harness().await;
        let isn = 1000u32;
        harness
            .packets
            .send(tcp(50000, "198.51.100.20", 80, isn, None, true, b""))
            .await
            .unwrap();

        // SYN-ACK straight from the stack.
        let syn_ack = next_written(&mut harness).await;
        let parsed = SlicedPacket::from_ip(&syn_ack).unwrap();
        let Some(TransportSlice::Tcp(header)) = parsed.transport else {
            panic!("tcp")
        };
        assert!(header.syn() && header.ack());
        assert_eq!(header.acknowledgment_number(), isn + 1);
        let server_seq = header.sequence_number() + 1;

        harness
            .packets
            .send(tcp(
                50000,
                "198.51.100.20",
                80,
                isn + 1,
                Some(server_seq),
                false,
                b"hello",
            ))
            .await
            .unwrap();

        let request = timeout(Duration::from_secs(5), harness.requests.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(request.network, Network::Tcp);
        assert_eq!(request.destination, "198.51.100.20:80".parse().unwrap());

        // The echo comes back as data on the same connection.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(Instant::now() < deadline, "no data came back");
            let packet = next_written(&mut harness).await;
            let parsed = SlicedPacket::from_ip(&packet).unwrap();
            let Some(TransportSlice::Tcp(segment)) = parsed.transport else {
                continue;
            };
            if !segment.payload().is_empty() {
                assert_eq!(segment.payload(), b"hello");
                assert_eq!(segment.source_port(), 80);
                break;
            }
        }
    }

    #[tokio::test]
    async fn dns_over_tcp_to_any_address_is_sent_to_the_resolver() {
        let mut harness = harness().await;
        harness
            .packets
            .send(tcp(50001, "9.9.9.9", 53, 1, None, true, b""))
            .await
            .unwrap();
        let syn_ack = next_written(&mut harness).await;
        let parsed = SlicedPacket::from_ip(&syn_ack).unwrap();
        let Some(TransportSlice::Tcp(header)) = parsed.transport else {
            panic!("tcp")
        };
        harness
            .packets
            .send(tcp(
                50001,
                "9.9.9.9",
                53,
                2,
                Some(header.sequence_number() + 1),
                false,
                b"\x00\x01x",
            ))
            .await
            .unwrap();

        let request = timeout(Duration::from_secs(5), harness.requests.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(request.destination, "1.1.1.1:53".parse().unwrap());
    }
}
