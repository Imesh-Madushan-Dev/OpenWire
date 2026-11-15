//! UDP flows from the tunnel service.
//!
//! Each one is a single source talking to a single destination, carried as
//! length-prefixed datagrams (see [`crate::core::tun::wire`]). Out through
//! the tunnel, VLESS's UDP framing is identical, so its flows reuse the TCP
//! relay as they are; Trojan names the address on every datagram, so its
//! flows are re-framed both ways. Destinations the LAN bypass keeps local
//! get a real UDP socket.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use tokio::io::{AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::{TcpStream, UdpSocket};
use tokio::time::timeout;

use super::{inbound, is_local_destination, relay, trojan, vless, IoStream, Session, IDLE_TIMEOUT};
use crate::core::geo;
use crate::core::models::Outbound;
use crate::core::net;
use crate::core::tun::wire::{self, Status};
use crate::core::usage::{ConnRecorder, TrafficKind, UsageKey};
use crate::error::{AppError, AppResult};

enum Upstream {
    Direct(UdpSocket),
    Vless(Box<dyn IoStream>),
    Trojan(Box<dyn IoStream>),
}

pub(super) async fn handle(
    mut client: TcpStream,
    request: inbound::Request,
    app: String,
    session: Arc<Session>,
) -> AppResult<()> {
    let target = request
        .host
        .parse::<IpAddr>()
        .map(|ip| SocketAddr::new(ip, request.port))
        .map_err(|_| AppError::Protocol("a UDP flow without an address".into()))?;

    let upstream = match open(&session, &request, target).await {
        Ok(upstream) => upstream,
        Err(err) => {
            let _ = inbound::send_tun_status(&mut client, Status::Failed).await;
            return Err(err);
        }
    };
    inbound::send_tun_status(&mut client, Status::Ok).await?;

    let usage = session.usage.open(
        UsageKey {
            app,
            host: request.host.clone(),
            kind: TrafficKind::from_port(request.port),
            country: geo::country_of_literal(&request.host),
        },
        Arc::clone(&session.metrics),
    );
    let recorder = Arc::clone(&usage.recorder);

    match upstream {
        // VLESS answers with its 2+N byte head first, as for TCP.
        Upstream::Vless(stream) => relay(client, stream, recorder, Vec::new(), true).await,
        Upstream::Trojan(stream) => relay_trojan(client, stream, target, recorder).await,
        Upstream::Direct(socket) => relay_direct(client, socket, recorder).await,
    }
}

async fn open(
    session: &Session,
    request: &inbound::Request,
    target: SocketAddr,
) -> AppResult<Upstream> {
    let direct = (session.lan_bypass && is_local_destination(&request.host))
        || matches!(session.node.outbound, Outbound::Direct);
    if direct {
        return net::udp_to(target)
            .await
            .map(Upstream::Direct)
            .map_err(|e| AppError::Dial(format!("udp {target}: {e}")));
    }

    let dialer = session
        .dialer
        .as_ref()
        .ok_or_else(|| AppError::Internal("dialer missing".into()))?;
    match &session.node.outbound {
        Outbound::Vless(config) => vless::dial_udp(dialer, config, &request.host, request.port)
            .await
            .map(Upstream::Vless),
        Outbound::Trojan(config) => trojan::dial_udp(dialer, config, &request.host, request.port)
            .await
            .map(Upstream::Trojan),
        Outbound::Direct => Err(AppError::Internal("outbound/dialer mismatch".into())),
    }
}

fn io(e: std::io::Error) -> AppError {
    AppError::Protocol(format!("udp relay: {e}"))
}

/// Either direction ending ends the flow: UDP has no half-close, and the
/// service closes an idle flow from its side.
async fn relay_trojan(
    client: TcpStream,
    upstream: Box<dyn IoStream>,
    target: SocketAddr,
    recorder: Arc<ConnRecorder>,
) -> AppResult<()> {
    let (mut client_read, mut client_write) = client.into_split();
    let (mut server_read, mut server_write): (ReadHalf<_>, WriteHalf<_>) =
        tokio::io::split(upstream);
    let host = target.ip().to_string();

    let up = async {
        let mut datagram = Vec::new();
        while let Ok(Ok(true)) = timeout(
            IDLE_TIMEOUT,
            wire::read_datagram(&mut client_read, &mut datagram),
        )
        .await
        {
            let packet = trojan::udp_packet(&host, target.port(), &datagram).map_err(io)?;
            server_write.write_all(&packet).await.map_err(io)?;
            server_write.flush().await.map_err(io)?;
            recorder.add_up(datagram.len() as u64);
        }
        let _ = server_write.shutdown().await;
        Ok(())
    };
    let down = async {
        let mut payload = Vec::new();
        while let Ok(Ok(true)) = timeout(
            IDLE_TIMEOUT,
            trojan::read_udp_packet(&mut server_read, &mut payload),
        )
        .await
        {
            wire::write_datagram(&mut client_write, &payload)
                .await
                .map_err(io)?;
            recorder.add_down(payload.len() as u64);
        }
        Ok(())
    };

    tokio::select! {
        result = up => result,
        result = down => result,
    }
}

async fn relay_direct(
    client: TcpStream,
    socket: UdpSocket,
    recorder: Arc<ConnRecorder>,
) -> AppResult<()> {
    let (mut client_read, mut client_write) = client.into_split();

    let up = async {
        let mut datagram = Vec::new();
        while let Ok(Ok(true)) = timeout(
            IDLE_TIMEOUT,
            wire::read_datagram(&mut client_read, &mut datagram),
        )
        .await
        {
            socket.send(&datagram).await.map_err(io)?;
            recorder.add_up(datagram.len() as u64);
        }
        Ok(())
    };
    let down = async {
        let mut buffer = vec![0u8; wire::MAX_DATAGRAM];
        while let Ok(Ok(read)) = timeout(IDLE_TIMEOUT, socket.recv(&mut buffer)).await {
            wire::write_datagram(&mut client_write, &buffer[..read])
                .await
                .map_err(io)?;
            recorder.add_down(read as u64);
        }
        Ok(())
    };

    tokio::select! {
        result = up => result,
        result = down => result,
    }
}
