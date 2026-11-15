//! The service's side of [`super::wire`]: opening a flow on the app's proxy.

use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::time::timeout;

use super::wire::{self, FlowRequest, Status};

/// Loopback answers at once; this only guards against a proxy that has hung.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// The proxy answers once its own leg to the destination is up, which
/// crosses the server - the same bound the proxy itself uses for a dial.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(25);

/// A stream carrying `request`, ready for its bytes or datagrams.
pub async fn open(proxy: SocketAddr, request: FlowRequest) -> io::Result<TcpStream> {
    let mut stream = timeout(CONNECT_TIMEOUT, TcpStream::connect(proxy))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "local proxy did not accept"))??;
    let _ = stream.set_nodelay(true);

    tokio::io::AsyncWriteExt::write_all(&mut stream, &request.handshake()).await?;
    let status = timeout(ANSWER_TIMEOUT, wire::read_acceptance(&mut stream))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "local proxy did not answer"))??;

    match status {
        Status::Ok => Ok(stream),
        Status::Blocked => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "held by the kill switch",
        )),
        Status::Failed => Err(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            format!("{} unreachable", request.destination),
        )),
    }
}
