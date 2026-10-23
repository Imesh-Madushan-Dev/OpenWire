//! WebSocket (RFC 6455), client side, for the `ws` transport: the upgrade
//! handshake, then every write as one masked binary frame and every data
//! frame's payload read back as plain bytes. No extensions, no text.

use std::io;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, ReadBuf};

use super::{http1, random, Outbox};
use crate::core::tunnel::IoStream;
use crate::error::{AppError, AppResult};

const ACCEPT_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
/// Raw bytes held from the server while frame headers are parsed.
const READ_BUFFER: usize = 32 * 1024;

const OP_CONTINUATION: u8 = 0x0;
const OP_TEXT: u8 = 0x1;
const OP_BINARY: u8 = 0x2;
const OP_CLOSE: u8 = 0x8;
const OP_PING: u8 = 0x9;
const OP_PONG: u8 = 0xA;

/// Upgrades `stream` to a WebSocket at `path`. `early`, the start of the
/// protocol head, rides base64'd in `Sec-WebSocket-Protocol` - where Xray
/// reads early data from - so the server can dial out before the upgrade is
/// even answered.
pub async fn connect<S: IoStream>(
    stream: S,
    path: &str,
    host: &str,
    early: &[u8],
) -> AppResult<WsStream<BufReader<S>>> {
    let mut nonce = [0u8; 16];
    random(&mut nonce);
    let key = STANDARD.encode(nonce);
    let early = URL_SAFE_NO_PAD.encode(early);

    let mut headers = vec![
        ("Connection", "Upgrade"),
        ("Upgrade", "websocket"),
        ("Sec-WebSocket-Version", "13"),
        ("Sec-WebSocket-Key", key.as_str()),
    ];
    if !early.is_empty() {
        headers.push(("Sec-WebSocket-Protocol", early.as_str()));
    }

    let mut stream = BufReader::new(stream);
    let io = |e: io::Error| AppError::Protocol(format!("websocket handshake: {e}"));
    stream
        .write_all(&http1::request("GET", path, host, &headers))
        .await
        .map_err(io)?;
    stream.flush().await.map_err(io)?;

    let head = http1::read_head(&mut stream).await?;
    if head.status != 101 {
        return Err(AppError::Protocol(format!(
            "the server refused the WebSocket upgrade (HTTP {}) - check the path",
            head.status
        )));
    }
    if head.header("sec-websocket-accept") != Some(accept_key(&key).as_str()) {
        return Err(AppError::Protocol(
            "the server's WebSocket handshake is not valid".into(),
        ));
    }
    Ok(WsStream::new(stream))
}

/// What the server must answer a key with, proving it read the request.
fn accept_key(key: &str) -> String {
    let digest = ring::digest::digest(
        &ring::digest::SHA1_FOR_LEGACY_USE_ONLY,
        format!("{key}{ACCEPT_GUID}").as_bytes(),
    );
    STANDARD.encode(digest.as_ref())
}

/// An upgraded connection.
pub struct WsStream<S> {
    inner: S,
    buffer: Box<[u8]>,
    start: usize,
    end: usize,
    /// Payload bytes of the current data frame not read out yet.
    remaining: u64,
    /// Servers never mask, but a masked frame still reads right: the key,
    /// and how far into the payload it has been applied.
    mask: Option<([u8; 4], usize)>,
    closed: bool,
    outbox: Outbox,
}

impl<S> WsStream<S> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            buffer: vec![0; READ_BUFFER].into_boxed_slice(),
            start: 0,
            end: 0,
            remaining: 0,
            mask: None,
            closed: false,
            outbox: Outbox::default(),
        }
    }
}

struct FrameHeader {
    opcode: u8,
    len: u64,
    mask: Option<[u8; 4]>,
    /// Bytes the header itself takes.
    size: usize,
}

/// A frame header from the front of `bytes`, once all of it has arrived.
fn parse_header(bytes: &[u8]) -> Option<FrameHeader> {
    let [first, second, ..] = *bytes else {
        return None;
    };
    let (len, mut size) = match second & 0x7f {
        126 => (
            u64::from(u16::from_be_bytes(bytes.get(2..4)?.try_into().ok()?)),
            4,
        ),
        127 => (u64::from_be_bytes(bytes.get(2..10)?.try_into().ok()?), 10),
        len => (u64::from(len), 2),
    };
    let mask = if second & 0x80 != 0 {
        let key = bytes.get(size..size + 4)?.try_into().ok()?;
        size += 4;
        Some(key)
    } else {
        None
    };
    Some(FrameHeader {
        opcode: first & 0x0f,
        len,
        mask,
        size,
    })
}

/// Appends one final frame, masked: a client must mask everything it sends.
fn encode_frame(out: &mut Vec<u8>, opcode: u8, payload: &[u8]) {
    out.push(0x80 | opcode);
    match payload.len() {
        len @ 0..=125 => out.push(0x80 | len as u8),
        len @ 126..=0xffff => {
            out.push(0x80 | 126);
            out.extend_from_slice(&(len as u16).to_be_bytes());
        }
        len => {
            out.push(0x80 | 127);
            out.extend_from_slice(&(len as u64).to_be_bytes());
        }
    }
    let mut key = [0u8; 4];
    random(&mut key);
    out.extend_from_slice(&key);
    let start = out.len();
    out.extend_from_slice(payload);
    apply_mask(&mut out[start..], key, 0);
}

fn apply_mask(bytes: &mut [u8], key: [u8; 4], offset: usize) {
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte ^= key[(offset + i) & 3];
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncRead for WsStream<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        loop {
            if this.closed {
                return Poll::Ready(Ok(()));
            }
            let buffered = this.end - this.start;

            if this.remaining > 0 {
                if buffered > 0 {
                    let n = buffered
                        .min(buf.remaining())
                        .min(usize::try_from(this.remaining).unwrap_or(usize::MAX));
                    let chunk = &mut this.buffer[this.start..this.start + n];
                    if let Some((key, offset)) = &mut this.mask {
                        apply_mask(chunk, *key, *offset);
                        *offset += n;
                    }
                    buf.put_slice(chunk);
                    this.start += n;
                    this.remaining -= n as u64;
                    return Poll::Ready(Ok(()));
                }
            } else if let Some(header) = parse_header(&this.buffer[this.start..this.end]) {
                match header.opcode {
                    OP_CONTINUATION | OP_TEXT | OP_BINARY => {
                        this.start += header.size;
                        this.remaining = header.len;
                        this.mask = header.mask.map(|key| (key, 0));
                        continue;
                    }
                    OP_CLOSE => {
                        this.closed = true;
                        continue;
                    }
                    OP_PING | OP_PONG => {
                        if header.len > 125 {
                            return Poll::Ready(Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "oversized WebSocket control frame",
                            )));
                        }
                        let total = header.size + header.len as usize;
                        if buffered >= total {
                            if header.opcode == OP_PING {
                                let from = this.start + header.size;
                                let mut payload = this.buffer[from..this.start + total].to_vec();
                                if let Some(key) = header.mask {
                                    apply_mask(&mut payload, key, 0);
                                }
                                encode_frame(this.outbox.queue(), OP_PONG, &payload);
                                // Best effort now; whatever is left goes out
                                // ahead of the next write.
                                let _ = this.outbox.poll_drain(&mut this.inner, cx);
                            }
                            this.start += total;
                            continue;
                        }
                    }
                    _ => {
                        return Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "unknown WebSocket frame",
                        )))
                    }
                }
            }

            // Nothing complete to hand out: read more from the server.
            if this.start == this.end {
                this.start = 0;
                this.end = 0;
            } else if this.start > 0 {
                this.buffer.copy_within(this.start..this.end, 0);
                this.end -= this.start;
                this.start = 0;
            }
            let mut read = ReadBuf::new(&mut this.buffer[this.end..]);
            ready!(Pin::new(&mut this.inner).poll_read(cx, &mut read))?;
            match read.filled().len() {
                // The server hung up without a close frame.
                0 => this.closed = true,
                n => this.end += n,
            }
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWrite for WsStream<S> {
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
                encode_frame(out, OP_BINARY, data)
            })
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        ready!(this.outbox.poll_drain(&mut this.inner, cx))?;
        Pin::new(&mut this.inner).poll_flush(cx)
    }

    /// Sends no close frame. A WebSocket cannot half-close, and closing it
    /// would cut off a download still in flight; the connection ends when
    /// the stream is dropped, as Xray's own client leaves it.
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    /// The worked example from RFC 6455 section 1.3.
    #[test]
    fn the_accept_key_matches_the_rfc() {
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    fn server_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
        let mut frame = vec![0x80 | opcode];
        match payload.len() {
            len @ 0..=125 => frame.push(len as u8),
            len => {
                frame.push(126);
                frame.extend_from_slice(&(len as u16).to_be_bytes());
            }
        }
        frame.extend_from_slice(payload);
        frame
    }

    /// Unmasks what the client wrote, frame by frame.
    fn client_payloads(mut wire: &[u8]) -> Vec<(u8, Vec<u8>)> {
        let mut frames = Vec::new();
        while let Some(header) = parse_header(wire) {
            let key = header.mask.expect("client frames are masked");
            let mut payload = wire[header.size..header.size + header.len as usize].to_vec();
            apply_mask(&mut payload, key, 0);
            frames.push((header.opcode, payload));
            wire = &wire[header.size + header.len as usize..];
        }
        frames
    }

    #[tokio::test]
    async fn payloads_read_back_across_frames_and_pings_get_ponged() {
        let (mut server, client) = tokio::io::duplex(64 * 1024);
        let mut ws = WsStream::new(client);

        let big = vec![7u8; 300];
        let mut wire = server_frame(OP_BINARY, b"hello ");
        wire.extend(server_frame(OP_PING, b"are you there"));
        wire.extend(server_frame(OP_BINARY, &big));
        wire.extend(server_frame(OP_CLOSE, b""));
        // Dribbled out a few bytes at a time, so headers arrive split.
        let feeder = tokio::spawn(async move {
            for piece in wire.chunks(5) {
                server.write_all(piece).await.unwrap();
            }
            server
        });

        let mut read = Vec::new();
        ws.read_to_end(&mut read).await.unwrap();
        let mut expected = b"hello ".to_vec();
        expected.extend(&big);
        assert_eq!(read, expected);

        ws.write_all(b"upload").await.unwrap();
        ws.flush().await.unwrap();
        let mut server = feeder.await.unwrap();
        drop(ws);
        let mut wire = Vec::new();
        server.read_to_end(&mut wire).await.unwrap();
        assert_eq!(
            client_payloads(&wire),
            vec![
                (OP_PONG, b"are you there".to_vec()),
                (OP_BINARY, b"upload".to_vec())
            ]
        );
    }

    #[tokio::test]
    async fn the_upgrade_carries_early_data_and_checks_the_answer() {
        let (server, client) = tokio::io::duplex(64 * 1024);
        let answer = tokio::spawn(async move {
            let mut server = BufReader::new(server);
            let mut request = Vec::new();
            loop {
                let mut line = Vec::new();
                tokio::io::AsyncBufReadExt::read_until(&mut server, b'\n', &mut line)
                    .await
                    .unwrap();
                request.extend_from_slice(&line);
                if line == b"\r\n" {
                    break;
                }
            }
            let request = String::from_utf8(request).unwrap();
            let key = request
                .lines()
                .find_map(|l| l.strip_prefix("Sec-WebSocket-Key: "))
                .unwrap()
                .to_string();
            server
                .write_all(
                    format!(
                        "HTTP/1.1 101 Switching Protocols\r\nSec-WebSocket-Accept: {}\r\n\r\n",
                        accept_key(&key)
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            request
        });

        connect(client, "/ws", "cdn.example.com", b"head")
            .await
            .expect("upgrade");
        let request = answer.await.unwrap();
        assert!(request.starts_with("GET /ws HTTP/1.1\r\nHost: cdn.example.com\r\n"));
        assert!(request.contains(&format!(
            "Sec-WebSocket-Protocol: {}\r\n",
            URL_SAFE_NO_PAD.encode(b"head")
        )));
    }
}
