//! Release check: the newest OpenWire published on GitHub, and whether it is
//! newer than this build.
//!
//! Reads one redirect rather than the REST API. `/releases/latest` on
//! github.com answers 302 to the newest published release's tag page, so the
//! response headers carry everything needed: no JSON, no chunked body to
//! decode, and no API quota - 60 requests an hour per address, which a
//! carrier-grade NAT shares with thousands of strangers. Drafts and
//! pre-releases are never "latest", so neither is ever offered.
//!
//! The request goes straight out, not through the tunnel, and says nothing
//! about the user beyond the app's version in its User-Agent.

use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore};
use semver::Version;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;

use crate::core::models::UpdateStatus;
use crate::error::{AppError, AppResult};

/// Where releases are published. The About page opens the same repository.
const REPO: &str = "Imesh-Madushan-Dev/OpenWire";
const HOST: &str = "github.com";
const TIMEOUT: Duration = Duration::from_secs(15);
/// GitHub's headers run to about 6 KB, most of it the content security policy.
const MAX_HEADERS: usize = 64 * 1024;

/// This build's version.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Where things stand before any check has finished.
pub fn unchecked() -> UpdateStatus {
    UpdateStatus {
        current: CURRENT.to_string(),
        latest: None,
        available: false,
        checked_at: None,
    }
}

/// The outcome of a check that found `tag`, or no release at all.
pub fn status(tag: Option<&str>, checked_at: u64) -> UpdateStatus {
    UpdateStatus {
        current: CURRENT.to_string(),
        latest: tag.map(|tag| version_of(tag).to_string()),
        available: tag.is_some_and(|tag| is_newer(tag, CURRENT)),
        checked_at: Some(checked_at),
    }
}

/// The tag of the newest published release, e.g. `v0.3.0`; `None` while the
/// repository has none.
pub async fn latest_tag() -> AppResult<Option<String>> {
    let headers = timeout(TIMEOUT, fetch_headers())
        .await
        .map_err(|_| AppError::Update("timed out".into()))??;
    parse_latest(&headers)
}

/// Whether `tag` names a later version than `current`. Tags are semver with
/// an optional `v`; one that does not parse is never offered.
pub fn is_newer(tag: &str, current: &str) -> bool {
    match (Version::parse(version_of(tag)), Version::parse(version_of(current))) {
        (Ok(latest), Ok(current)) => latest > current,
        _ => false,
    }
}

fn version_of(tag: &str) -> &str {
    tag.trim().trim_start_matches(['v', 'V'])
}

/// Asks for the latest release and returns the response up to the end of its
/// headers; the body is never needed.
async fn fetch_headers() -> AppResult<String> {
    let tcp = TcpStream::connect((HOST, 443))
        .await
        .map_err(|e| AppError::Update(format!("dial {HOST}: {e}")))?;
    let name = ServerName::try_from(HOST).map_err(|e| AppError::Update(e.to_string()))?;
    let mut stream = TlsConnector::from(client_config()?)
        .connect(name, tcp)
        .await
        .map_err(|e| AppError::Update(format!("tls: {e}")))?;

    let request = format!(
        "GET /{REPO}/releases/latest HTTP/1.1\r\n\
         Host: {HOST}\r\n\
         User-Agent: OpenWire/{CURRENT}\r\n\
         Connection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| AppError::Update(format!("send: {e}")))?;

    let mut received = Vec::with_capacity(8 * 1024);
    let mut chunk = [0u8; 4096];
    loop {
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|e| AppError::Update(format!("read: {e}")))?;
        if read == 0 {
            break;
        }
        received.extend_from_slice(&chunk[..read]);
        if let Some(end) = received.windows(4).position(|w| w == b"\r\n\r\n") {
            received.truncate(end);
            break;
        }
        if received.len() > MAX_HEADERS {
            return Err(AppError::Update("response headers too long".into()));
        }
    }
    Ok(String::from_utf8_lossy(&received).into_owned())
}

/// Ordinary certificate checking against the bundled roots - unlike the
/// tunnel's, there is no second name to accept.
fn client_config() -> AppResult<Arc<ClientConfig>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let roots = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let mut config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| AppError::Update(e.to_string()))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

/// The tag a `/releases/latest` response redirects to: the last segment of
/// `/releases/tag/<tag>`. A redirect anywhere else - the release list, while
/// nothing is published - means there is no release yet.
fn parse_latest(headers: &str) -> AppResult<Option<String>> {
    let mut lines = headers.lines();
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or_default();
    if !status.starts_with('3') {
        return Err(AppError::Update(format!("unexpected status {status:?}")));
    }

    let location = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("location"))
        .map(|(_, value)| value.trim())
        .ok_or_else(|| AppError::Update("redirect without a location".into()))?;

    Ok(location
        .split_once("/releases/tag/")
        .and_then(|(_, rest)| rest.split(['?', '#', '/']).next())
        .filter(|tag| !tag.is_empty())
        .map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redirect(location: &str) -> String {
        format!(
            "HTTP/1.1 302 Found\r\nContent-Type: text/html\r\nLocation: {location}\r\nContent-Length: 0"
        )
    }

    #[test]
    fn reads_the_tag_from_the_redirect() {
        let headers = redirect("https://github.com/Imesh-Madushan-Dev/OpenWire/releases/tag/v0.2.0");
        assert_eq!(parse_latest(&headers).unwrap().as_deref(), Some("v0.2.0"));
    }

    #[test]
    fn header_names_are_case_insensitive_and_queries_are_dropped() {
        let headers = "HTTP/1.1 301 Moved\r\nlocation: /o/r/releases/tag/v1.0.0?from=latest";
        assert_eq!(parse_latest(headers).unwrap().as_deref(), Some("v1.0.0"));
    }

    #[test]
    fn a_repository_without_releases_has_no_latest() {
        let headers = redirect("https://github.com/Imesh-Madushan-Dev/OpenWire/releases");
        assert_eq!(parse_latest(&headers).unwrap(), None);
    }

    #[test]
    fn anything_but_a_redirect_is_an_error() {
        assert!(parse_latest("HTTP/1.1 404 Not Found\r\nContent-Length: 9").is_err());
        assert!(parse_latest("HTTP/1.1 429 Too Many Requests").is_err());
        assert!(parse_latest("HTTP/1.1 302 Found\r\nContent-Length: 0").is_err());
        assert!(parse_latest("").is_err());
    }

    #[test]
    fn compares_versions_rather_than_strings() {
        assert!(is_newer("v0.3.0", "0.2.0"));
        assert!(is_newer("v0.10.0", "0.9.0"));
        assert!(is_newer("1.0.0", "1.0.0-beta.2"));
        assert!(!is_newer("v0.2.0", "0.2.0"));
        assert!(!is_newer("v0.1.0", "0.2.0"));
        assert!(!is_newer("nightly", "0.2.0"));
    }

    #[test]
    fn status_reports_the_version_without_its_prefix() {
        let found = status(Some("v999.0.0"), 1);
        assert_eq!(found.latest.as_deref(), Some("999.0.0"));
        assert!(found.available);
        assert_eq!(found.checked_at, Some(1));

        let current = status(Some(&format!("v{CURRENT}")), 1);
        assert!(!current.available);

        assert!(!status(None, 1).available);
    }

    #[tokio::test]
    #[ignore = "reaches github.com"]
    async fn finds_the_published_release() {
        let tag = latest_tag().await.expect("GitHub answered").expect("a release");
        assert!(Version::parse(version_of(&tag)).is_ok(), "{tag} is not semver");
    }
}
