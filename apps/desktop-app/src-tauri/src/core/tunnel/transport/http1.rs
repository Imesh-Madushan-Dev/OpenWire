//! The slice of HTTP/1.1 the WebSocket, HTTPUpgrade, XHTTP and fake-header
//! transports need: request heads, response heads and chunked bodies. Enough
//! to talk to a proxy server, not a general client.

use std::future::poll_fn;
use std::io;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

use tokio::io::{AsyncBufRead, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, ReadBuf};

use super::Outbox;
use crate::core::tunnel::IoStream;
use crate::error::{AppError, AppResult};

/// What Xray's own client sends: a current Chrome on Windows.
pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/151.0.0.0 Safari/537.36";

/// The rest of a browser-looking request for the `headerType=http` disguise,
/// after `Host` and `User-Agent` - Xray's defaults.
pub const FAKE_HEADERS: &[(&str, &str)] = &[
    ("Accept-Encoding", "gzip, deflate"),
    ("Connection", "keep-alive"),
    ("Pragma", "no-cache"),
];

/// A response head bigger than this is not from a proxy server.
const MAX_HEAD: usize = 16 * 1024;

/// A request head: the request line, `Host`, a browser's `User-Agent`, then
/// `headers` in order.
pub fn request(method: &str, path: &str, host: &str, headers: &[(&str, &str)]) -> Vec<u8> {
    let mut head =
        format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: {USER_AGENT}\r\n");
    for (name, value) in headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    head.into_bytes()
}

#[derive(Debug)]
pub struct Head {
    pub status: u16,
    /// Names lowercased.
    headers: Vec<(String, String)>,
}

impl Head {
    /// A header's value, by its lowercase name.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, value)| value.as_str())
    }
}

/// Reads a response head, stopping exactly at the blank line after it, so
/// whatever the server sent next stays in `reader`.
pub async fn read_head<R: AsyncBufRead + Unpin>(reader: &mut R) -> AppResult<Head> {
    let mut parser = HeadParser::default();
    poll_fn(|cx| parser.poll(Pin::new(&mut *reader), cx))
        .await
        .map_err(|e| AppError::Protocol(format!("read response: {e}")))
}

/// HTTPUpgrade: asks to upgrade to a WebSocket and, once the server says
/// 101, uses the connection as it is - no WebSocket framing at all.
pub async fn upgrade<S: IoStream>(stream: S, path: &str, host: &str) -> AppResult<BufReader<S>> {
    let mut stream = BufReader::new(stream);
    let io = |e: io::Error| AppError::Protocol(format!("http upgrade: {e}"));
    stream
        .write_all(&request(
            "GET",
            path,
            host,
            &[("Connection", "Upgrade"), ("Upgrade", "websocket")],
        ))
        .await
        .map_err(io)?;
    stream.flush().await.map_err(io)?;

    let head = read_head(&mut stream).await?;
    if head.status != 101 {
        return Err(AppError::Protocol(format!(
            "the server refused the upgrade (HTTP {}) - check the path",
            head.status
        )));
    }
    Ok(stream)
}

#[derive(Default)]
struct HeadParser {
    line: Vec<u8>,
    size: usize,
    status: Option<u16>,
    headers: Vec<(String, String)>,
}

impl HeadParser {
    fn poll<R: AsyncBufRead + ?Sized>(
        &mut self,
        mut reader: Pin<&mut R>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<Head>> {
        loop {
            ready!(poll_line(reader.as_mut(), cx, &mut self.line))?;
            self.size += self.line.len();
            if self.size > MAX_HEAD {
                return Poll::Ready(Err(invalid("response head too long")));
            }
            let line = String::from_utf8_lossy(&self.line).trim_end().to_string();
            self.line.clear();

            match self.status {
                None => {
                    self.status =
                        Some(parse_status(&line).ok_or_else(|| invalid("not an HTTP response"))?)
                }
                Some(status) if line.is_empty() => {
                    return Poll::Ready(Ok(Head {
                        status,
                        headers: std::mem::take(&mut self.headers),
                    }))
                }
                Some(_) => {
                    if let Some((name, value)) = line.split_once(':') {
                        self.headers
                            .push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
                    }
                }
            }
        }
    }
}

/// `HTTP/1.1 101 Switching Protocols` -> 101.
fn parse_status(line: &str) -> Option<u16> {
    line.strip_prefix("HTTP/1.")?.get(2..5)?.parse().ok()
}

/// Moves bytes from `reader` into `line`, up to and including the next `\n`.
fn poll_line<R: AsyncBufRead + ?Sized>(
    mut reader: Pin<&mut R>,
    cx: &mut Context<'_>,
    line: &mut Vec<u8>,
) -> Poll<io::Result<()>> {
    loop {
        let available = ready!(reader.as_mut().poll_fill_buf(cx))?;
        if available.is_empty() {
            return Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into()));
        }
        let (used, done) = match available.iter().position(|&b| b == b'\n') {
            Some(at) => (at + 1, true),
            None => (available.len(), false),
        };
        line.extend_from_slice(&available[..used]);
        reader.as_mut().consume(used);
        if done {
            return Poll::Ready(Ok(()));
        }
        if line.len() > MAX_HEAD {
            return Poll::Ready(Err(invalid("line too long")));
        }
    }
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

/// A response body as a byte stream. The head is read on the first read,
/// not up front: an XHTTP server may hold its answer until it has data.
pub struct Body<R> {
    inner: R,
    state: BodyState,
    head: HeadParser,
    line: Vec<u8>,
}

#[derive(Clone, Copy)]
enum BodyState {
    Head,
    ChunkSize,
    Chunk(u64),
    ChunkEnd,
    Trailers,
    Length(u64),
    /// No length given: the body runs until the server closes.
    ToEnd,
    Done,
}

impl<R> Body<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            state: BodyState::Head,
            head: HeadParser::default(),
            line: Vec::new(),
        }
    }
}

impl<R: AsyncBufRead + AsyncRead + Unpin> AsyncRead for Body<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        loop {
            match this.state {
                BodyState::Head => {
                    let head = ready!(this.head.poll(Pin::new(&mut this.inner), cx))?;
                    if head.status != 200 {
                        return Poll::Ready(Err(io::Error::other(format!(
                            "the server answered HTTP {}",
                            head.status
                        ))));
                    }
                    let chunked = head
                        .header("transfer-encoding")
                        .is_some_and(|v| v.to_ascii_lowercase().contains("chunked"));
                    this.state = if chunked {
                        BodyState::ChunkSize
                    } else {
                        match head.header("content-length").and_then(|v| v.parse().ok()) {
                            Some(length) => BodyState::Length(length),
                            None => BodyState::ToEnd,
                        }
                    };
                }
                BodyState::ChunkSize => {
                    ready!(poll_line(Pin::new(&mut this.inner), cx, &mut this.line))?;
                    let size = String::from_utf8_lossy(&this.line)
                        .split(';')
                        .next()
                        .and_then(|size| u64::from_str_radix(size.trim(), 16).ok())
                        .ok_or_else(|| invalid("bad chunk size"))?;
                    this.line.clear();
                    this.state = if size == 0 {
                        BodyState::Trailers
                    } else {
                        BodyState::Chunk(size)
                    };
                }
                BodyState::Chunk(left) | BodyState::Length(left) => {
                    if left == 0 {
                        this.state = BodyState::Done;
                        continue;
                    }
                    let available = ready!(Pin::new(&mut this.inner).poll_fill_buf(cx))?;
                    if available.is_empty() {
                        return Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into()));
                    }
                    let n = available
                        .len()
                        .min(buf.remaining())
                        .min(usize::try_from(left).unwrap_or(usize::MAX));
                    buf.put_slice(&available[..n]);
                    Pin::new(&mut this.inner).consume(n);

                    let left = left - n as u64;
                    this.state = match this.state {
                        BodyState::Chunk(_) if left == 0 => BodyState::ChunkEnd,
                        BodyState::Chunk(_) => BodyState::Chunk(left),
                        _ => BodyState::Length(left),
                    };
                    return Poll::Ready(Ok(()));
                }
                BodyState::ChunkEnd => {
                    ready!(poll_line(Pin::new(&mut this.inner), cx, &mut this.line))?;
                    this.line.clear();
                    this.state = BodyState::ChunkSize;
                }
                BodyState::Trailers => {
                    ready!(poll_line(Pin::new(&mut this.inner), cx, &mut this.line))?;
                    let blank = this.line == b"\r\n" || this.line == b"\n";
                    this.line.clear();
                    if blank {
                        this.state = BodyState::Done;
                    }
                }
                BodyState::ToEnd => return Pin::new(&mut this.inner).poll_read(cx, buf),
                BodyState::Done => return Poll::Ready(Ok(())),
            }
        }
    }
}

/// A request body sent in chunks, for an upload of unknown length. Shutting
/// down sends the last chunk - ending the body, not the connection, which may
/// still be carrying the response.
pub struct Chunked<W> {
    inner: W,
    outbox: Outbox,
    ended: bool,
}

impl<W> Chunked<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            outbox: Outbox::default(),
            ended: false,
        }
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for Chunked<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        this.outbox
            .poll_write(&mut this.inner, cx, buf, |out, data| {
                out.extend_from_slice(format!("{:x}\r\n", data.len()).as_bytes());
                out.extend_from_slice(data);
                out.extend_from_slice(b"\r\n");
            })
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        ready!(this.outbox.poll_drain(&mut this.inner, cx))?;
        Pin::new(&mut this.inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if !this.ended {
            this.ended = true;
            this.outbox.queue().extend_from_slice(b"0\r\n\r\n");
        }
        ready!(this.outbox.poll_drain(&mut this.inner, cx))?;
        Pin::new(&mut this.inner).poll_flush(cx)
    }
}

/// Drops the fake response head a `headerType=http` server sends ahead of
/// its first bytes. Everything after it, and every write, passes through.
pub struct SkipHead<S> {
    inner: S,
    /// How much of `\r\n\r\n` the bytes so far end with; 4 once it is past.
    matched: u8,
    skipped: usize,
}

impl<S> SkipHead<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            matched: 0,
            skipped: 0,
        }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for SkipHead<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        while this.matched < 4 {
            let start = buf.filled().len();
            ready!(Pin::new(&mut this.inner).poll_read(cx, buf))?;
            let read = &buf.filled()[start..];
            if read.is_empty() {
                return Poll::Ready(Ok(()));
            }

            let mut head_end = None;
            for (at, &byte) in read.iter().enumerate() {
                this.matched = match (this.matched, byte) {
                    (2, b'\r') => 3,
                    (_, b'\r') => 1,
                    (1, b'\n') => 2,
                    (3, b'\n') => 4,
                    _ => 0,
                };
                if this.matched == 4 {
                    head_end = Some(at + 1);
                    break;
                }
            }

            let rest = head_end.map(|at| read[at..].to_vec()).unwrap_or_default();
            this.skipped += read.len() - rest.len();
            buf.set_filled(start);
            if this.skipped > MAX_HEAD {
                return Poll::Ready(Err(invalid("response head too long")));
            }
            if !rest.is_empty() {
                buf.put_slice(&rest);
                return Poll::Ready(Ok(()));
            }
        }
        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for SkipHead<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    async fn read_all<R: AsyncRead + Unpin>(mut reader: R) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        reader.read_to_end(&mut out).await?;
        Ok(out)
    }

    #[tokio::test]
    async fn a_head_stops_at_the_blank_line() {
        let mut reader = BufReader::new(
            &b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nSec-WebSocket-Accept: abc\r\n\r\nrest"[..],
        );
        let head = read_head(&mut reader).await.unwrap();
        assert_eq!(head.status, 101);
        assert_eq!(head.header("sec-websocket-accept"), Some("abc"));
        assert_eq!(read_all(reader).await.unwrap(), b"rest");
    }

    #[tokio::test]
    async fn chunked_bodies_decode_across_their_framing() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n7;ext=1\r\n, world\r\n0\r\nX-Trailer: 1\r\n\r\n";
        let body = Body::new(BufReader::with_capacity(3, &raw[..]));
        assert_eq!(read_all(body).await.unwrap(), b"hello, world");

        let sized = b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nbodyignored";
        assert_eq!(
            read_all(Body::new(BufReader::new(&sized[..])))
                .await
                .unwrap(),
            b"body"
        );

        let refused = b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
        assert!(read_all(Body::new(BufReader::new(&refused[..])))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn chunked_uploads_end_with_the_last_chunk() {
        let mut out = Vec::new();
        {
            let mut writer = Chunked::new(&mut out);
            writer.write_all(b"hello").await.unwrap();
            writer.write_all(&[0u8; 20]).await.unwrap();
            writer.shutdown().await.unwrap();
        }
        let mut expected = b"5\r\nhello\r\n14\r\n".to_vec();
        expected.extend_from_slice(&[0u8; 20]);
        expected.extend_from_slice(b"\r\n0\r\n\r\n");
        assert_eq!(out, expected);
    }

    #[tokio::test]
    async fn the_fake_response_head_is_dropped_wherever_reads_split_it() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: video/mpeg\r\n\r\npayload";
        for size in [1, 3, 7, raw.len()] {
            let (mut tx, rx) = tokio::io::duplex(size);
            let writer = tokio::spawn(async move {
                for piece in raw.chunks(size) {
                    tx.write_all(piece).await.unwrap();
                }
            });
            assert_eq!(
                read_all(SkipHead::new(rx)).await.unwrap(),
                b"payload",
                "{size}"
            );
            writer.await.unwrap();
        }
    }

    #[test]
    fn requests_carry_host_and_a_browser_user_agent() {
        let head = request("GET", "/ws", "cdn.example.com", &[("Upgrade", "websocket")]);
        let text = String::from_utf8(head).unwrap();
        assert!(text.starts_with("GET /ws HTTP/1.1\r\nHost: cdn.example.com\r\n"));
        assert!(text.contains("User-Agent: Mozilla/5.0"));
        assert!(text.ends_with("Upgrade: websocket\r\n\r\n"));
    }
}
