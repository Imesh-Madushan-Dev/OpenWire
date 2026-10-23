//! XHTTP (SplitHTTP before it): the stream split over ordinary HTTP
//! requests, so it gets through CDNs and proxies that buffer or refuse
//! anything else. Requests look the way Xray's own client sends them.
//!
//! - stream-one: one POST whose request and response bodies are the stream.
//! - stream-up: a GET streams the download; one POST streams the upload.
//! - packet-up: a GET streams the download; the upload goes as numbered
//!   POSTs, which the server puts back in order.
//!
//! Over HTTP/2 each request is a stream on the session's connection. Over
//! HTTP/1.1 each streaming request gets a connection of its own, and the
//! upload POSTs share one, kept alive.

use std::time::Duration;

use bytes::Bytes;
use http::{Method, Request};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream};
use tokio::time::Instant;

use super::http2::{self, h2_error};
use super::{http1, random_u32, Dialer, Endpoint, Joined};
use crate::core::models::XhttpMode;
use crate::core::tunnel::IoStream;
use crate::error::{AppError, AppResult};

/// The link's `extra`, as far as the client side goes. Defaults are Xray's.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Length range of the `x_padding` every request carries. The server
    /// refuses requests whose padding falls outside its own range.
    padding: (u32, u32),
    /// The most one upload POST may carry; the server refuses bigger ones.
    max_post: usize,
    /// Upload POSTs are spaced at least this far apart, which batches small
    /// writes into fewer requests.
    min_post_interval: Duration,
    /// `Content-Type: application/grpc` on streaming uploads, which some
    /// CDNs need to pass them through unbuffered.
    grpc_header: bool,
    headers: Vec<(String, String)>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            padding: (100, 1000),
            max_post: 1_000_000,
            min_post_interval: Duration::from_millis(30),
            grpc_header: true,
            headers: Vec::new(),
        }
    }
}

impl Settings {
    /// Reads `extra`. Anything that would change the requests in a way this
    /// client doesn't follow is refused, so a server set up with it fails at
    /// import rather than on every connection.
    pub fn parse(extra: &str) -> AppResult<Self> {
        let mut settings = Self::default();
        if extra.trim().is_empty() {
            return Ok(settings);
        }
        let unsupported = |what: &str| {
            AppError::Invalid(format!(
                "This server's XHTTP settings use {what}, which OpenWire doesn't support yet."
            ))
        };
        let malformed = |key: &str| {
            AppError::Invalid(format!(
                "That link isn't a valid server link: its XHTTP `{key}` setting can't be read"
            ))
        };

        let value: Value = serde_json::from_str(extra).map_err(|_| malformed("extra"))?;
        let object = value.as_object().ok_or_else(|| malformed("extra"))?;
        for (key, value) in object {
            match key.as_str() {
                "xPaddingBytes" => settings.padding = range(value).ok_or_else(|| malformed(key))?,
                "scMaxEachPostBytes" => {
                    settings.max_post = range(value)
                        .map(|(from, _)| from as usize)
                        .filter(|&bytes| bytes > 0)
                        .ok_or_else(|| malformed(key))?
                }
                "scMinPostsIntervalMs" => {
                    settings.min_post_interval = range(value)
                        .map(|(from, _)| Duration::from_millis(u64::from(from)))
                        .ok_or_else(|| malformed(key))?
                }
                "noGRPCHeader" => settings.grpc_header = !value.as_bool().unwrap_or(false),
                "headers" => {
                    let headers = value.as_object().ok_or_else(|| malformed(key))?;
                    settings.headers = headers
                        .iter()
                        .filter_map(|(name, value)| {
                            Some((name.clone(), value.as_str()?.to_string()))
                        })
                        .collect();
                }
                "downloadSettings" if !value.is_null() => {
                    return Err(unsupported("a separate download server"))
                }
                "uplinkHTTPMethod" if !is_default(value, "POST") => {
                    return Err(unsupported("a custom upload method"))
                }
                "sessionIDPlacement" | "seqPlacement" if !is_default(value, "path") => {
                    return Err(unsupported("session IDs outside the path"))
                }
                "uplinkDataPlacement" if !is_default(value, "body") => {
                    return Err(unsupported("upload data outside the request body"))
                }
                "sessionIDLength" | "sessionIDTable" if !value.is_null() => {
                    return Err(unsupported("custom session IDs"))
                }
                "xPaddingObfsMode" if value.as_bool() == Some(true) => {
                    return Err(unsupported("padding obfuscation"))
                }
                // Connection reuse (xmux) is the client's own business, and
                // the rest only configure the server.
                _ => {}
            }
        }
        Ok(settings)
    }
}

fn is_default(value: &Value, default: &str) -> bool {
    value.is_null()
        || value
            .as_str()
            .is_some_and(|v| v.is_empty() || v.eq_ignore_ascii_case(default))
}

/// Xray writes ranges as a number, `"from-to"`, or `{ "from", "to" }`.
fn range(value: &Value) -> Option<(u32, u32)> {
    let number = |value: &Value| value.as_u64().and_then(|n| u32::try_from(n).ok());
    let (from, to) = match value {
        Value::Number(_) => {
            let n = number(value)?;
            (n, n)
        }
        Value::String(text) => match text.split_once('-') {
            Some((from, to)) => (from.trim().parse().ok()?, to.trim().parse().ok()?),
            None => {
                let n = text.trim().parse().ok()?;
                (n, n)
            }
        },
        Value::Object(fields) => (number(fields.get("from")?)?, number(fields.get("to")?)?),
        _ => return None,
    };
    (from <= to).then_some((from, to))
}

/// What every request of one connection shares.
#[derive(Clone)]
struct Target {
    scheme: &'static str,
    host: String,
    /// The path, with a slash at each end.
    base: String,
    /// The link path's query, carried on every request.
    query: String,
    settings: Settings,
}

impl Target {
    fn new(endpoint: &Endpoint, path: &str, host: &str, settings: &Settings) -> Self {
        let (path, query) = path.split_once('?').unwrap_or((path, ""));
        let mut base = super::path_or_root(path);
        if !base.ends_with('/') {
            base.push('/');
        }
        Self {
            scheme: endpoint.scheme(),
            host: endpoint.authority(host),
            base,
            query: query.to_string(),
            settings: settings.clone(),
        }
    }

    /// `rest` under the base path, with the link's query.
    fn path(&self, rest: &str) -> String {
        let mut path = format!("{}{rest}", self.base);
        if !self.query.is_empty() {
            path.push('?');
            path.push_str(&self.query);
        }
        path
    }

    /// The headers of a Chrome `fetch()`, as Xray's client sends them, the
    /// link's own, and a `Referer` padded to a random length the server checks.
    fn headers(&self, streaming_upload: bool) -> Vec<(String, String)> {
        let (low, high) = self.settings.padding;
        let span = u64::from(high - low) + 1;
        let padding = low as usize + (u64::from(random_u32()) % span) as usize;
        let referer = format!(
            "{}://{}{}?x_padding={}",
            self.scheme,
            self.host,
            self.base,
            "X".repeat(padding)
        );

        let mut headers = vec![
            ("Accept", "*/*"),
            ("Accept-Language", "en-US,en;q=0.9"),
            ("Cache-Control", "no-cache"),
        ];
        if streaming_upload && self.settings.grpc_header {
            headers.push(("Content-Type", "application/grpc"));
        }
        headers.extend([
            ("DNT", "1"),
            ("Pragma", "no-cache"),
            ("Priority", "u=1, i"),
            ("Referer", referer.as_str()),
            (
                "Sec-CH-UA",
                r#""Not=A?Brand";v="99", "Google Chrome";v="151", "Chromium";v="151""#,
            ),
            ("Sec-CH-UA-Mobile", "?0"),
            ("Sec-CH-UA-Platform", r#""Windows""#),
            ("Sec-Fetch-Dest", "empty"),
            ("Sec-Fetch-Mode", "cors"),
            ("Sec-Fetch-Site", "same-origin"),
        ]);
        let mut headers: Vec<(String, String)> = headers
            .into_iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        headers.extend(self.settings.headers.iter().cloned());
        headers
    }

    fn h1_request(
        &self,
        method: &str,
        path: &str,
        mut headers: Vec<(String, String)>,
        extra: &[(&str, &str)],
    ) -> Vec<u8> {
        headers.extend(extra.iter().map(|(n, v)| (n.to_string(), v.to_string())));
        let pairs: Vec<(&str, &str)> = headers
            .iter()
            .map(|(n, v)| (n.as_str(), v.as_str()))
            .collect();
        http1::request(method, path, &self.host, &pairs)
    }

    fn h2_request(
        &self,
        method: Method,
        path: &str,
        streaming_upload: bool,
    ) -> AppResult<Request<()>> {
        let mut request = Request::builder()
            .method(method)
            .uri(format!("{}://{}{path}", self.scheme, self.host))
            .header("user-agent", http1::USER_AGENT);
        for (name, value) in self.headers(streaming_upload) {
            request = request.header(name, value);
        }
        request.body(()).map_err(|e| {
            AppError::Invalid(format!(
                "This server's XHTTP path or headers can't be used: {e}"
            ))
        })
    }
}

pub async fn open(
    dialer: &Dialer,
    path: &str,
    host: &str,
    mode: XhttpMode,
) -> AppResult<Box<dyn IoStream>> {
    let settings = dialer
        .xhttp
        .as_ref()
        .ok_or_else(|| AppError::Internal("xhttp settings missing".into()))?;
    let target = Target::new(&dialer.endpoint, path, host, settings);
    let session = uuid::Uuid::new_v4().to_string();
    // Auto is packet-up, which works through anything.
    let mode = match mode {
        XhttpMode::Auto => XhttpMode::PacketUp,
        other => other,
    };
    if dialer.xhttp_h2 {
        over_h2(dialer, target, &session, mode).await
    } else {
        over_h1(&dialer.endpoint, target, &session, mode).await
    }
}

async fn over_h2(
    dialer: &Dialer,
    target: Target,
    session: &str,
    mode: XhttpMode,
) -> AppResult<Box<dyn IoStream>> {
    let (pool, endpoint) = (&dialer.h2, &dialer.endpoint);

    if mode == XhttpMode::StreamOne {
        let path = target.path("");
        let (response, send) = pool
            .send(
                endpoint,
                || target.h2_request(Method::POST, &path, true),
                false,
            )
            .await?;
        return Ok(Box::new(Joined::new(
            http2::Reader::new(response, false),
            http2::Writer::new(send, false),
        )));
    }

    let path = target.path(session);
    let (download, _) = pool
        .send(
            endpoint,
            || target.h2_request(Method::GET, &path, false),
            true,
        )
        .await?;
    let reader = http2::Reader::new(download, false);

    if mode == XhttpMode::StreamUp {
        let (answer, send) = pool
            .send(
                endpoint,
                || target.h2_request(Method::POST, &path, true),
                false,
            )
            .await?;
        // The upload's answer only comes once it has ended.
        tokio::spawn(drain_h2(answer));
        return Ok(Box::new(Joined::new(
            reader,
            http2::Writer::new(send, false),
        )));
    }

    let handle = pool.ready(endpoint).await?;
    let (writer, pipe) = tokio::io::duplex(target.settings.max_post);
    tokio::spawn(upload_packets(
        pipe,
        Poster::H2(handle),
        target,
        session.to_string(),
    ));
    Ok(Box::new(Joined::new(reader, writer)))
}

async fn over_h1(
    endpoint: &Endpoint,
    target: Target,
    session: &str,
    mode: XhttpMode,
) -> AppResult<Box<dyn IoStream>> {
    let io = |e: std::io::Error| AppError::Protocol(format!("xhttp request: {e}"));
    // `Connection: close` on the streaming requests, as Xray's client sends
    // it: told the connection is kept, Go's server reads and throws away up
    // to 256 KB of a body still arriving before it writes its answer.
    let streaming_post = |path: &str| {
        target.h1_request(
            "POST",
            path,
            target.headers(true),
            &[("Transfer-Encoding", "chunked"), ("Connection", "close")],
        )
    };

    if mode == XhttpMode::StreamOne {
        let mut conn = endpoint.connect().await?;
        conn.write_all(&streaming_post(&target.path("")))
            .await
            .map_err(io)?;
        let (read, write) = tokio::io::split(conn);
        return Ok(Box::new(Joined::new(
            http1::Body::new(BufReader::new(read)),
            http1::Chunked::new(write),
        )));
    }

    let path = target.path(session);
    let mut download = endpoint.connect().await?;
    download
        .write_all(&target.h1_request(
            "GET",
            &path,
            target.headers(false),
            &[("Connection", "close")],
        ))
        .await
        .map_err(io)?;
    download.flush().await.map_err(io)?;
    let reader = http1::Body::new(BufReader::new(download));

    if mode == XhttpMode::StreamUp {
        let mut upload = endpoint.connect().await?;
        upload.write_all(&streaming_post(&path)).await.map_err(io)?;
        return Ok(Box::new(Joined::new(reader, http1::Chunked::new(upload))));
    }

    let (writer, pipe) = tokio::io::duplex(target.settings.max_post);
    tokio::spawn(upload_packets(
        pipe,
        Poster::H1 {
            endpoint: endpoint.clone(),
            conn: None,
            unanswered: false,
        },
        target,
        session.to_string(),
    ));
    Ok(Box::new(Joined::new(reader, writer)))
}

/// Sends the upload as numbered POSTs: whatever was written since the last
/// one, up to the server's limit. Ends when the tunnel stops writing.
async fn upload_packets(
    mut pipe: DuplexStream,
    mut poster: Poster,
    target: Target,
    session: String,
) {
    let mut buffer = vec![0u8; target.settings.max_post];
    let mut last = Instant::now();
    for seq in 0u64.. {
        // The head goes out as soon as dialing ends and the first payload
        // right behind it; pacing those two would hold up every
        // connection's first request.
        if seq >= 2 {
            tokio::time::sleep_until(last + target.settings.min_post_interval).await;
        }
        let n = match pipe.read(&mut buffer).await {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        last = Instant::now();

        let path = target.path(&format!("{session}/{seq}"));
        let body = Bytes::copy_from_slice(&buffer[..n]);
        if let Err(err) = poster.post(&target, &path, body).await {
            // Dropping the pipe fails the tunnel's next write, ending it.
            log::debug!("xhttp upload failed: {err}");
            return;
        }
    }
}

enum Poster {
    H2(::h2::client::SendRequest<Bytes>),
    /// One connection kept alive; each POST's answer is read just before the
    /// next goes out, as Xray's client does it.
    H1 {
        endpoint: Endpoint,
        conn: Option<BufReader<Box<dyn IoStream>>>,
        unanswered: bool,
    },
}

impl Poster {
    async fn post(&mut self, target: &Target, path: &str, body: Bytes) -> AppResult<()> {
        let length = body.len().to_string();
        match self {
            Poster::H2(handle) => {
                let mut request = target.h2_request(Method::POST, path, false)?;
                request.headers_mut().insert(
                    http::header::CONTENT_LENGTH,
                    http::HeaderValue::from(body.len()),
                );
                let (answer, mut send) = handle
                    .clone()
                    .ready()
                    .await
                    .map_err(h2_error)?
                    .send_request(request, false)
                    .map_err(h2_error)?;
                send.send_data(body, true).map_err(h2_error)?;
                tokio::spawn(drain_h2(answer));
                Ok(())
            }
            Poster::H1 {
                endpoint,
                conn,
                unanswered,
            } => {
                let mut request = target.h1_request(
                    "POST",
                    path,
                    target.headers(false),
                    &[("Content-Length", &length)],
                );
                request.extend_from_slice(&body);

                // A kept-alive connection the server has since closed is
                // normal; that gets one fresh connection.
                for _ in 0..2 {
                    if conn.is_none() {
                        *conn = Some(BufReader::new(endpoint.connect().await?));
                        *unanswered = false;
                    }
                    let Some(stream) = conn.as_mut() else {
                        continue;
                    };
                    if *unanswered {
                        if read_answer(stream).await.is_err() {
                            *conn = None;
                            continue;
                        }
                        *unanswered = false;
                    }
                    if stream.write_all(&request).await.is_ok() && stream.flush().await.is_ok() {
                        *unanswered = true;
                        return Ok(());
                    }
                    *conn = None;
                }
                Err(AppError::Protocol(
                    "the upload connection keeps closing".into(),
                ))
            }
        }
    }
}

/// Reads one upload POST's answer off a kept-alive connection.
async fn read_answer(stream: &mut BufReader<Box<dyn IoStream>>) -> AppResult<()> {
    let head = http1::read_head(stream).await?;
    if head.status != 200 {
        return Err(AppError::Protocol(format!(
            "the server refused an upload (HTTP {})",
            head.status
        )));
    }
    let length = head
        .header("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    tokio::io::copy(&mut (&mut *stream).take(length), &mut tokio::io::sink())
        .await
        .map_err(|e| AppError::Protocol(format!("read upload answer: {e}")))?;
    Ok(())
}

/// Waits out an upload request's answer, so its stream closes cleanly.
async fn drain_h2(answer: ::h2::client::ResponseFuture) {
    match answer.await {
        Ok(response) => {
            if !response.status().is_success() {
                log::debug!("xhttp upload refused: HTTP {}", response.status().as_u16());
            }
            let mut body = response.into_body();
            while let Some(Ok(data)) = body.data().await {
                let _ = body.flow_control().release_capacity(data.len());
            }
        }
        Err(err) => log::debug!("xhttp upload failed: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_settings_are_read_or_refused() {
        let settings = Settings::parse(
            r#"{"xPaddingBytes":"200-300","scMaxEachPostBytes":500000,"noGRPCHeader":true,
                "headers":{"X-Edge":"1"},"xmux":{"maxConcurrency":"16-32"},"sessionIDPlacement":"path"}"#,
        )
        .unwrap();
        assert_eq!(settings.padding, (200, 300));
        assert_eq!(settings.max_post, 500_000);
        assert!(!settings.grpc_header);
        assert_eq!(
            settings.headers,
            vec![("X-Edge".to_string(), "1".to_string())]
        );

        assert!(Settings::parse("").is_ok());
        for refused in [
            r#"{"downloadSettings":{"address":"dl.example.com"}}"#,
            r#"{"sessionIDPlacement":"header"}"#,
            r#"{"xPaddingObfsMode":true}"#,
            r#"{"uplinkHTTPMethod":"PUT"}"#,
            r#"{"xPaddingBytes":"300-200"}"#,
            "not json",
        ] {
            assert!(Settings::parse(refused).is_err(), "{refused}");
        }
    }

    #[test]
    fn requests_go_under_the_base_path_with_padding_in_range() {
        let endpoint = Endpoint {
            host: "203.0.113.10".into(),
            port: 80,
            tls: None,
        };
        let target = Target::new(&endpoint, "xh?k=v", "cdn.example.com", &Settings::default());
        assert_eq!(target.path("abc/0"), "/xh/abc/0?k=v");

        let headers = target.headers(true);
        let referer = &headers.iter().find(|(n, _)| n == "Referer").unwrap().1;
        let padding = referer
            .strip_prefix("http://cdn.example.com/xh/?x_padding=")
            .unwrap();
        assert!((100..=1000).contains(&padding.len()), "{}", padding.len());
        assert!(headers
            .iter()
            .any(|(n, v)| n == "Content-Type" && v == "application/grpc"));
    }
}
