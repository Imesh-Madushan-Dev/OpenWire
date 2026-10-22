//! The TLS layer every masked outbound shares.
//!
//! The camouflage is the split between two names:
//!   * the TCP connection goes to `node.host` (the VPS), and
//!   * the TLS ClientHello carries the configured SNI (a whitelisted service).
//!
//! On the wire, a DPI middlebox sees a TLS session to the whitelisted name and
//! nothing else - the real destination, and which protocol carries it, live
//! inside the encrypted payload. VLESS and Trojan differ only in what they
//! write once this handshake is done.

use std::sync::Arc;
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

use crate::core::models::TlsSettings;
use crate::error::{AppError, AppResult};

const DIAL_TIMEOUT: Duration = Duration::from_secs(12);

/// Built once per session - assembling the root store is expensive and must not
/// happen per connection.
/// `endpoint_host` is the real TCP destination, which is deliberately not the
/// name sent in the ClientHello.
pub fn build(config: TlsSettings<'_>, endpoint_host: &str) -> AppResult<Arc<ClientConfig>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let mut tls = if config.allow_insecure {
        // Opt-in only. Without a verified certificate the camouflage still hides
        // the destination, but an on-path attacker could impersonate the exit.
        log::warn!("certificate verification disabled for this server");
        ClientConfig::builder_with_provider(Arc::clone(&provider))
            .with_safe_default_protocol_versions()
            .map_err(|e| AppError::Tls(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipVerification(provider)))
            .with_no_client_auth()
    } else {
        let roots = Arc::new(RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        });
        let webpki = WebPkiServerVerifier::builder_with_provider(roots, Arc::clone(&provider))
            .build()
            .map_err(|e| AppError::Tls(e.to_string()))?;

        let names = verification_names(config, endpoint_host);
        if names.is_empty() {
            return Err(AppError::Invalid(
                "This server has no valid hostname to verify against.".into(),
            ));
        }

        ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| AppError::Tls(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(EitherName {
                inner: webpki,
                names,
            }))
            .with_no_client_auth()
    };

    // ALPN is part of the disguise: it should match what the impersonated
    // service would offer.
    tls.alpn_protocols = config.alpn.iter().map(|p| p.as_bytes().to_vec()).collect();

    Ok(Arc::new(tls))
}

/// Dials the endpoint and completes the masked handshake.
pub async fn connect(
    node_host: &str,
    node_port: u16,
    sni: &str,
    tls: Arc<ClientConfig>,
) -> AppResult<TlsStream<TcpStream>> {
    // The SNI we present, not the host we dial. Never collapse these two.
    let sni = ServerName::try_from(sni.to_string()).map_err(|_| {
        AppError::Invalid("The masking domain for this server is not valid.".into())
    })?;

    let tcp = timeout(DIAL_TIMEOUT, TcpStream::connect((node_host, node_port)))
        .await
        .map_err(|_| AppError::Dial(format!("{node_host}:{node_port} timed out")))?
        .map_err(|e| AppError::Dial(format!("{node_host}:{node_port}: {e}")))?;
    // Interactive traffic: latency matters far more than packet efficiency.
    let _ = tcp.set_nodelay(true);

    timeout(DIAL_TIMEOUT, TlsConnector::from(tls).connect(sni, tcp))
        .await
        .map_err(|_| AppError::Tls("handshake timed out".into()))?
        .map_err(|e| AppError::Tls(e.to_string()))
}

/// The first flight of the handshake [`connect`] would make - the ClientHello,
/// with the same SNI and ALPN - for the latency probe to send and time.
///
/// Nothing past the server's first reply byte is ever read, so its
/// certificate is never seen; verification would have nothing to check.
pub fn client_hello(sni: &str, alpn: &[String]) -> Option<Vec<u8>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = ClientConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .ok()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipVerification(provider)))
        .with_no_client_auth();
    config.alpn_protocols = alpn.iter().map(|p| p.as_bytes().to_vec()).collect();

    let name = ServerName::try_from(sni.to_string()).ok()?;
    let mut connection = rustls::ClientConnection::new(Arc::new(config), name).ok()?;
    let mut hello = Vec::new();
    connection.write_tls(&mut hello).ok()?;
    Some(hello)
}

/// Names a certificate may legitimately be issued for.
///
/// A masked server proves the name it actually owns (the endpoint), while the
/// ClientHello carries the decoy. Checking only the decoy would reject every
/// self-hosted node; checking only the endpoint would reject a genuinely
/// CDN-fronted one. Both names came from the user's own link, so either proof
/// authenticates the peer they meant to reach - and unlike `allow_insecure`,
/// this is still real authentication.
fn verification_names(config: TlsSettings<'_>, endpoint_host: &str) -> Vec<ServerName<'static>> {
    let mut names: Vec<ServerName<'static>> = Vec::new();
    if let Ok(name) = ServerName::try_from(endpoint_host.to_string()) {
        names.push(name);
    }
    if let Ok(name) = ServerName::try_from(config.sni.to_string()) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Delegates to the real webpki verifier, but accepts the certificate if it is
/// valid for any one of the configured names rather than only the requested one.
#[derive(Debug)]
struct EitherName {
    inner: Arc<WebPkiServerVerifier>,
    names: Vec<ServerName<'static>>,
}

impl ServerCertVerifier for EitherName {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        // Ignored: this is the decoy SNI, which the server cannot prove.
        _server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let mut last_error = None;
        for name in &self.names {
            // Chain, expiry and revocation are all still enforced here; only the
            // name being matched against differs.
            match self
                .inner
                .verify_server_cert(end_entity, intermediates, name, ocsp_response, now)
            {
                Ok(verified) => return Ok(verified),
                Err(err) => last_error = Some(err),
            }
        }
        Err(last_error.unwrap_or(rustls::Error::General(
            "no configured name to verify against".into(),
        )))
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

#[derive(Debug)]
struct SkipVerification(Arc<rustls::crypto::CryptoProvider>);

impl ServerCertVerifier for SkipVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        // The signature itself is still checked; only the name/chain is skipped.
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn masked(alpn: &[String], allow_insecure: bool) -> TlsSettings<'_> {
        TlsSettings {
            sni: "aka.ms",
            alpn,
            allow_insecure,
        }
    }

    fn h2() -> Vec<String> {
        vec!["h2".into()]
    }

    #[test]
    fn tls_config_offers_the_configured_alpn() {
        let alpn = h2();
        let tls = build(masked(&alpn, false), "edge.example.org").expect("tls config");
        assert_eq!(tls.alpn_protocols, vec![b"h2".to_vec()]);
    }

    /// The verifier must consider the real endpoint as well as the decoy SNI -
    /// a masked server can only ever prove the name it actually owns.
    #[test]
    fn verifier_accepts_either_configured_name() {
        let alpn = h2();
        let names = verification_names(masked(&alpn, false), "edge.example.org");
        assert_eq!(names.len(), 2);
        assert!(names.contains(&ServerName::try_from("edge.example.org").unwrap()));
        assert!(names.contains(&ServerName::try_from("aka.ms").unwrap()));

        // When the two names agree there is nothing extra to check.
        let plain = TlsSettings {
            sni: "edge.example.org",
            ..masked(&alpn, false)
        };
        assert_eq!(verification_names(plain, "edge.example.org").len(), 1);
    }

    #[test]
    fn insecure_config_skips_the_name_check_entirely() {
        let alpn = h2();
        assert!(build(masked(&alpn, true), "edge.example.org").is_ok());
    }
}
