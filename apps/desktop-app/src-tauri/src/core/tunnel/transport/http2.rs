//! HTTP/2, for the gRPC, h2 and XHTTP transports. One connection to the
//! server per session, with every tunneled connection a stream on it, so a
//! new one costs no handshake at all. Plain HTTP/2 (h2c) when there is no TLS.

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{ready, Context, Poll};
use std::time::Duration;

use ::h2::client::{self, ResponseFuture, SendRequest};
use ::h2::{Ping, RecvStream, SendStream};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use http::{Method, Request, StatusCode};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::Mutex;
use tokio::time::timeout;

use super::{http1, Endpoint, Joined};
use crate::core::tunnel::IoStream;
use crate::error::{AppError, AppResult};

/// Receive windows big enough that a far server is not held to a trickle
/// waiting on window updates.
const STREAM_WINDOW: u32 = 1 << 20;
const CONNECTION_WINDOW: u32 = 8 << 20;
/// A connection that stops answering pings is dropped, so a network change
/// cannot leave every new stream waiting on a dead socket.
const KEEPALIVE: Duration = Duration::from_secs(15);
const KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(10);
/// A gRPC message bigger than this is not from a proxy server.
const MAX_GRPC_MESSAGE: usize = 4 << 20;
/// What grpc-go, which Xray and v2fly both use, calls itself.
const GRPC_USER_AGENT: &str = "grpc-go/1.71.1";

/// The session's connection, opened on first use and replaced once it dies.
#[derive(Default)]
pub struct Pool {
    slot: Mutex<Option<SendRequest<Bytes>>>,
}

impl Pool {
    /// Sends a request on the session's connection. A pooled connection can
    /// look alive and still fail the request, so that gets one retry on a
    /// fresh connection.
    pub async fn send(
        &self,
        endpoint: &Endpoint,
        request: impl Fn() -> AppResult<Request<()>>,
        end_of_stream: bool,
    ) -> AppResult<(ResponseFuture, SendStream<Bytes>)> {
        let mut retried = false;
        loop {
            let mut handle = self.ready(endpoint).await?;
            match handle.send_request(request()?, end_of_stream) {
                Ok(sent) => return Ok(sent),
                Err(err) if !retried => {
                    log::debug!("pooled http/2 connection failed ({err}); reconnecting");
                    *self.slot.lock().await = None;
                    retried = true;
                }
                Err(err) => return Err(h2_error(err)),
            }
        }
    }

    /// A handle with room for one more stream, on the pooled connection or
    /// a new one.
    pub async fn ready(&self, endpoint: &Endpoint) -> AppResult<SendRequest<Bytes>> {
        let mut slot = self.slot.lock().await;
        if let Some(handle) = slot.clone() {
            match handle.ready().await {
                Ok(handle) => return Ok(handle),
                Err(err) => log::debug!("http/2 connection closed: {err}"),
            }
        }

        let io = endpoint.connect_h2().await?;
        let (handle, connection) = client::Builder::new()
            .initial_window_size(STREAM_WINDOW)
            .initial_connection_window_size(CONNECTION_WINDOW)
            .handshake::<_, Bytes>(io)
            .await
            .map_err(h2_error)?;
        tokio::spawn(drive(connection));
        *slot = Some(handle.clone());
        handle.ready().await.map_err(h2_error)
    }
}

/// Runs the connection until it closes - when the session lets go of it,
/// the server does, or it stops answering pings.
async fn drive(mut connection: client::Connection<Box<dyn IoStream>, Bytes>) {
    let pings = connection.ping_pong();
    let keepalive = async move {
        let Some(mut pings) = pings else {
            return std::future::pending::<()>().await;
        };
        loop {
            tokio::time::sleep(KEEPALIVE).await;
            if !matches!(
                timeout(KEEPALIVE_TIMEOUT, pings.ping(Ping::opaque())).await,
                Ok(Ok(_))
            ) {
                return;
            }
        }
    };
    tokio::pin!(connection);
    tokio::select! {
        closed = &mut connection => {
            if let Err(err) = closed {
                log::debug!("http/2 connection ended: {err}");
            }
        }
        () = keepalive => log::debug!("http/2 connection stopped answering pings; dropped it"),
    }
}

/// gRPC "gun", as Xray and v2fly speak it: one `POST /<service>/Tun` whose
/// messages each carry one chunk of the stream.
pub async fn grpc(
    pool: &Pool,
    endpoint: &Endpoint,
    service_name: &str,
    authority: &str,
    multi: bool,
) -> AppResult<Box<dyn IoStream>> {
    let uri = format!(
        "{}://{}{}",
        endpoint.scheme(),
        endpoint.authority(authority),
        grpc_path(service_name, multi)
    );
    let (response, send) = pool
        .send(
            endpoint,
            || {
                Request::builder()
                    .method(Method::POST)
                    .uri(&uri)
                    .header("content-type", "application/grpc")
                    .header("te", "trailers")
                    .header("user-agent", GRPC_USER_AGENT)
                    .body(())
                    .map_err(invalid_request)
            },
            false,
        )
        .await?;
    Ok(Box::new(Joined::new(
        Reader::new(response, true),
        Writer::new(send, true),
    )))
}

/// The h2 transport (`type=http`): one `PUT` whose request and response
/// bodies are the two directions, as v2fly and sing-box serve it.
pub async fn stream(
    pool: &Pool,
    endpoint: &Endpoint,
    path: &str,
    host: &str,
) -> AppResult<Box<dyn IoStream>> {
    let uri = format!("{}://{}{path}", endpoint.scheme(), endpoint.authority(host));
    let (response, send) = pool
        .send(
            endpoint,
            || {
                Request::builder()
                    .method(Method::PUT)
                    .uri(&uri)
                    // Compressed answers would be bytes the tunnel can't use.
                    .header("accept-encoding", "identity")
                    .header("user-agent", http1::USER_AGENT)
                    .body(())
                    .map_err(invalid_request)
            },
            false,
        )
        .await?;
    Ok(Box::new(Joined::new(
        Reader::new(response, false),
        Writer::new(send, false),
    )))
}

/// `/<service>/Tun` (or `TunMulti`). A service name starting with `/` is
/// Xray's custom form, a whole path: `/a/b/Tun|TunMulti` puts the stream
/// names in the last segment.
fn grpc_path(service_name: &str, multi: bool) -> String {
    if !service_name.starts_with('/') {
        let stream = if multi { "TunMulti" } else { "Tun" };
        return format!("/{}/{stream}", path_escape(service_name));
    }
    let last = service_name.rfind('/').unwrap_or(0);
    let service = service_name[1..last.max(1)]
        .split('/')
        .map(path_escape)
        .collect::<Vec<_>>()
        .join("/");
    let names: Vec<&str> = service_name[last + 1..].split('|').collect();
    let stream = if multi && names.len() > 1 {
        names[1]
    } else {
        names[0]
    };
    format!("/{service}/{}", path_escape(stream))
}

/// Percent-encodes what may not appear in one path segment, as Go's
/// `url.PathEscape` does.
fn path_escape(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn invalid_request(err: http::Error) -> AppError {
    AppError::Invalid(format!(
        "This server's address or path can't be used in a request: {err}"
    ))
}

pub fn h2_error(err: ::h2::Error) -> AppError {
    AppError::Protocol(format!("http/2: {err}"))
}

fn io_error(err: ::h2::Error) -> io::Error {
    if err.is_io() {
        err.into_io().unwrap_or_else(|| io::ErrorKind::Other.into())
    } else {
        io::Error::other(err)
    }
}

/// A response body as bytes. The response is awaited on the first read, not
/// when the request goes out: a gRPC server sends its headers only with its
/// first message, and that may wait on the destination.
pub struct Reader {
    state: ReaderState,
    grpc: bool,
    /// gRPC bytes not yet a whole message.
    raw: BytesMut,
    /// Bytes ready to hand out.
    chunk: Bytes,
}

enum ReaderState {
    Waiting(ResponseFuture),
    Body(RecvStream),
    Done,
}

impl Reader {
    pub fn new(response: ResponseFuture, grpc: bool) -> Self {
        Self {
            state: ReaderState::Waiting(response),
            grpc,
            raw: BytesMut::new(),
            chunk: Bytes::new(),
        }
    }
}

impl AsyncRead for Reader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        loop {
            if !this.chunk.is_empty() {
                let n = this.chunk.len().min(buf.remaining());
                buf.put_slice(&this.chunk[..n]);
                this.chunk.advance(n);
                return Poll::Ready(Ok(()));
            }
            if this.grpc {
                if let Some(data) = grpc_decode(&mut this.raw)? {
                    this.chunk = data;
                    continue;
                }
            }

            match &mut this.state {
                ReaderState::Waiting(response) => {
                    let response = ready!(Pin::new(response).poll(cx)).map_err(io_error)?;
                    if response.status() != StatusCode::OK {
                        return Poll::Ready(Err(io::Error::other(format!(
                            "the server answered HTTP {}",
                            response.status().as_u16()
                        ))));
                    }
                    // A gRPC error with no body puts its status in the headers.
                    if let Some(status) = response.headers().get("grpc-status") {
                        if status.as_bytes() != b"0" {
                            return Poll::Ready(Err(io::Error::other(format!(
                                "the server answered gRPC status {}",
                                String::from_utf8_lossy(status.as_bytes())
                            ))));
                        }
                    }
                    this.state = ReaderState::Body(response.into_body());
                }
                ReaderState::Body(body) => match ready!(body.poll_data(cx)) {
                    Some(Ok(data)) => {
                        let _ = body.flow_control().release_capacity(data.len());
                        if this.grpc {
                            this.raw.extend_from_slice(&data);
                        } else {
                            this.chunk = data;
                        }
                    }
                    Some(Err(err)) => return Poll::Ready(Err(io_error(err))),
                    None => this.state = ReaderState::Done,
                },
                ReaderState::Done => return Poll::Ready(Ok(())),
            }
        }
    }
}

/// A request body fed from writes. Shutting down ends the body: a half-close
/// the server reads as the upload being over.
pub struct Writer {
    send: SendStream<Bytes>,
    grpc: bool,
    ended: bool,
}

impl Writer {
    pub fn new(send: SendStream<Bytes>, grpc: bool) -> Self {
        Self {
            send,
            grpc,
            ended: false,
        }
    }
}

impl AsyncWrite for Writer {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        // Waiting for window keeps a slow server from making the tunnel
        // buffer without bound.
        this.send.reserve_capacity(buf.len());
        let capacity = loop {
            let available = this.send.capacity();
            if available > 0 {
                break available;
            }
            match ready!(this.send.poll_capacity(cx)) {
                Some(Ok(_)) => {}
                Some(Err(err)) => return Poll::Ready(Err(io_error(err))),
                None => return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into())),
            }
        };

        let n = capacity.min(buf.len());
        let data = if this.grpc {
            grpc_encode(&buf[..n])
        } else {
            Bytes::copy_from_slice(&buf[..n])
        };
        this.send.send_data(data, false).map_err(io_error)?;
        Poll::Ready(Ok(n))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // The connection task writes frames out as soon as they are queued.
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if !this.ended {
            this.ended = true;
            this.send.send_data(Bytes::new(), true).map_err(io_error)?;
        }
        Poll::Ready(Ok(()))
    }
}

/// One gRPC message: `[uncompressed][length:u32be]` then the protobuf
/// `Hunk { bytes data = 1; }`.
fn grpc_encode(data: &[u8]) -> Bytes {
    let mut length = [0u8; 10];
    let length_len = put_varint(&mut length, data.len() as u64);
    let message_len = 1 + length_len + data.len();

    let mut out = BytesMut::with_capacity(5 + message_len);
    out.put_u8(0);
    out.put_u32(message_len as u32);
    out.put_u8(0x0a); // field 1, length-delimited
    out.put_slice(&length[..length_len]);
    out.put_slice(data);
    out.freeze()
}

/// Takes one whole gRPC message off the front of `raw`, if it has arrived,
/// and returns its data: a `Hunk`'s one field, or a `MultiHunk`'s several.
fn grpc_decode(raw: &mut BytesMut) -> io::Result<Option<Bytes>> {
    let bad = |what: &str| io::Error::new(io::ErrorKind::InvalidData, format!("grpc: {what}"));
    if raw.len() < 5 {
        return Ok(None);
    }
    if raw[0] != 0 {
        return Err(bad("compressed messages are not supported"));
    }
    let length = u32::from_be_bytes([raw[1], raw[2], raw[3], raw[4]]) as usize;
    if length > MAX_GRPC_MESSAGE {
        return Err(bad("message too large"));
    }
    if raw.len() < 5 + length {
        return Ok(None);
    }
    raw.advance(5);
    let message = raw.split_to(length).freeze();

    let mut parts: Vec<Bytes> = Vec::new();
    let mut at = 0;
    while at < message.len() {
        let (key, used) = read_varint(&message[at..]).ok_or_else(|| bad("bad field"))?;
        at += used;
        let skip = match key & 7 {
            0 => {
                read_varint(&message[at..])
                    .ok_or_else(|| bad("bad varint"))?
                    .1
            }
            1 => 8,
            2 => {
                let (len, used) = read_varint(&message[at..]).ok_or_else(|| bad("bad length"))?;
                at += used;
                let len = usize::try_from(len).map_err(|_| bad("bad length"))?;
                if message.len() - at < len {
                    return Err(bad("truncated field"));
                }
                if key >> 3 == 1 {
                    parts.push(message.slice(at..at + len));
                }
                len
            }
            5 => 4,
            _ => return Err(bad("unknown wire type")),
        };
        at = at
            .checked_add(skip)
            .filter(|&end| end <= message.len())
            .ok_or_else(|| bad("truncated field"))?;
    }

    Ok(Some(match parts.len() {
        0 => Bytes::new(),
        1 => parts.pop().unwrap_or_default(),
        _ => parts.concat().into(),
    }))
}

fn put_varint(out: &mut [u8; 10], mut value: u64) -> usize {
    let mut i = 0;
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out[i] = byte;
            return i + 1;
        }
        out[i] = byte | 0x80;
        i += 1;
    }
}

fn read_varint(bytes: &[u8]) -> Option<(u64, usize)> {
    let mut value = 0u64;
    for (i, &byte) in bytes.iter().enumerate().take(10) {
        value |= u64::from(byte & 0x7f) << (7 * i);
        if byte & 0x80 == 0 {
            return Some((value, i + 1));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grpc_messages_round_trip() {
        for size in [0, 1, 127, 128, 300, 70_000] {
            let data: Vec<u8> = (0..size).map(|i| i as u8).collect();
            let mut raw = BytesMut::from(&grpc_encode(&data)[..]);
            // Nothing comes out until the whole message is there.
            let mut partial = raw.split_to(raw.len() / 2);
            if size > 0 {
                assert!(grpc_decode(&mut partial).unwrap().is_none());
            }
            partial.unsplit(raw);
            assert_eq!(grpc_decode(&mut partial).unwrap().unwrap(), data, "{size}");
            assert!(partial.is_empty());
        }
    }

    #[test]
    fn a_multi_hunk_concatenates_and_unknown_fields_are_skipped() {
        // MultiHunk { data: "ab", data: "cd" } plus an unknown varint field 2.
        let message = [0x0a, 2, b'a', b'b', 0x10, 0x96, 0x01, 0x0a, 2, b'c', b'd'];
        let mut raw = BytesMut::new();
        raw.put_u8(0);
        raw.put_u32(message.len() as u32);
        raw.put_slice(&message);
        assert_eq!(&grpc_decode(&mut raw).unwrap().unwrap()[..], b"abcd");
    }

    #[test]
    fn grpc_paths_follow_xray() {
        assert_eq!(grpc_path("svc", false), "/svc/Tun");
        assert_eq!(grpc_path("svc", true), "/svc/TunMulti");
        assert_eq!(grpc_path("my svc", false), "/my%20svc/Tun");
        assert_eq!(
            grpc_path("/my/sample/path1|path2", false),
            "/my/sample/path1"
        );
        assert_eq!(
            grpc_path("/my/sample/path1|path2", true),
            "/my/sample/path2"
        );
        assert_eq!(grpc_path("/my/sample/path1", true), "/my/sample/path1");
    }
}
