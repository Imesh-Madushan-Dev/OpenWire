//! VLESS-over-TLS outbound with SNI camouflage.
//!
//! The camouflage is the split between two names:
//!   * the TCP connection goes to `node.host` (the VPS), and
//!   * the TLS ClientHello carries `VlessConfig::sni` (a whitelisted service).
//!
//! On the wire, a DPI middlebox sees a TLS session to the whitelisted name and
//! nothing else - the real destination lives inside the encrypted payload.

use std::sync::Arc;
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use uuid::Uuid;

use crate::core::models::VlessConfig;
use crate::error::{AppError, AppResult};

const DIAL_TIMEOUT: Duration = Duration::from_secs(12);

/// Built once per session - assembling the root store is expensive and must not
/// happen per connection.
pub fn build_tls(config: &VlessConfig) -> AppResult<Arc<ClientConfig>> {
    let roots = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };

    let mut tls = if config.allow_insecure {
        // Opt-in only. Without a verified certificate the camouflage still hides
        // the destination, but an on-path attacker could impersonate the exit.
        log::warn!("certificate verification disabled for this server");
        let provider = rustls::crypto::ring::default_provider();
        ClientConfig::builder_with_provider(Arc::new(provider.clone()))
            .with_safe_default_protocol_versions()
            .map_err(|e| AppError::Tls(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipVerification(Arc::new(provider))))
            .with_no_client_auth()
    } else {
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth()
    };

    // ALPN is part of the disguise: it should match what the impersonated
    // service would offer.
    tls.alpn_protocols = config
        .alpn
        .iter()
        .map(|p| p.as_bytes().to_vec())
        .collect();

    Ok(Arc::new(tls))
}

/// Opens the masked tunnel and writes the VLESS request head for `target`.
pub async fn dial(
    node_host: &str,
    node_port: u16,
    config: &VlessConfig,
    tls: Arc<ClientConfig>,
    target_host: &str,
    target_port: u16,
) -> AppResult<tokio_rustls::client::TlsStream<TcpStream>> {
    let uuid = Uuid::parse_str(&config.uuid)
        .map_err(|_| AppError::Invalid("The access key for this server is not a valid UUID.".into()))?;

    // The SNI we present, not the host we dial. Never collapse these two.
    let sni = ServerName::try_from(config.sni.clone())
        .map_err(|_| AppError::Invalid("The masking domain for this server is not valid.".into()))?;

    let tcp = timeout(DIAL_TIMEOUT, TcpStream::connect((node_host, node_port)))
        .await
        .map_err(|_| AppError::Dial(format!("{node_host}:{node_port} timed out")))?
        .map_err(|e| AppError::Dial(format!("{node_host}:{node_port}: {e}")))?;
    // Interactive traffic: latency matters far more than packet efficiency.
    let _ = tcp.set_nodelay(true);

    let mut stream = timeout(DIAL_TIMEOUT, TlsConnector::from(tls).connect(sni, tcp))
        .await
        .map_err(|_| AppError::Tls("handshake timed out".into()))?
        .map_err(|e| AppError::Tls(e.to_string()))?;

    stream
        .write_all(&request_head(uuid.as_bytes(), target_host, target_port))
        .await
        .map_err(|e| AppError::Protocol(format!("write vless head: {e}")))?;

    Ok(stream)
}

/// VLESS v0 request head:
/// `[ver=0][uuid:16][addon_len=0][cmd=1 tcp][port:u16be][atyp][addr]`
fn request_head(uuid: &[u8; 16], host: &str, port: u16) -> Vec<u8> {
    let mut head = Vec::with_capacity(32 + host.len());
    head.push(0x00); // version
    head.extend_from_slice(uuid);
    head.push(0x00); // no addons
    head.push(0x01); // TCP
    head.extend_from_slice(&port.to_be_bytes());

    // Note the address types are VLESS's, not SOCKS5's: 1=IPv4, 2=domain, 3=IPv6.
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(v4)) => {
            head.push(0x01);
            head.extend_from_slice(&v4.octets());
        }
        Ok(std::net::IpAddr::V6(v6)) => {
            head.push(0x03);
            head.extend_from_slice(&v6.octets());
        }
        Err(_) => {
            head.push(0x02);
            // Domain length is a single byte; longer names are not representable.
            let bytes = host.as_bytes();
            let len = bytes.len().min(255);
            head.push(len as u8);
            head.extend_from_slice(&bytes[..len]);
        }
    }
    head
}

/// Number of leading bytes of the VLESS response to discard:
/// `[ver][addon_len][addons...]`.
pub const RESPONSE_HEAD_MIN: usize = 2;

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

    fn uuid_bytes() -> [u8; 16] {
        Uuid::parse_str("2b4f8c1e-0000-4000-8000-abcdefabcdef")
            .unwrap()
            .into_bytes()
    }

    #[test]
    fn domain_head_matches_the_vless_layout() {
        let head = request_head(&uuid_bytes(), "example.com", 443);

        assert_eq!(head[0], 0x00, "version");
        assert_eq!(&head[1..17], &uuid_bytes(), "uuid");
        assert_eq!(head[17], 0x00, "addon length");
        assert_eq!(head[18], 0x01, "tcp command");
        assert_eq!(&head[19..21], &443u16.to_be_bytes(), "port");
        assert_eq!(head[21], 0x02, "domain address type");
        assert_eq!(head[22], 11, "domain length");
        assert_eq!(&head[23..], b"example.com");
    }

    #[test]
    fn ip_literals_use_the_binary_address_types() {
        let v4 = request_head(&uuid_bytes(), "203.0.113.10", 8443);
        assert_eq!(v4[21], 0x01);
        assert_eq!(&v4[22..26], &[203, 0, 113, 10]);
        assert_eq!(v4.len(), 26);

        let v6 = request_head(&uuid_bytes(), "::1", 443);
        assert_eq!(v6[21], 0x03);
        assert_eq!(v6.len(), 38);
    }

    #[test]
    fn overlong_domains_are_truncated_not_panicking() {
        let long = "a".repeat(400);
        let head = request_head(&uuid_bytes(), &long, 443);
        assert_eq!(head[22], 255);
    }

    #[test]
    fn tls_config_offers_the_configured_alpn() {
        let config = VlessConfig {
            uuid: "2b4f8c1e-0000-4000-8000-abcdefabcdef".into(),
            sni: "teams.microsoft.com".into(),
            alpn: vec!["h2".into()],
            allow_insecure: false,
        };
        let tls = build_tls(&config).expect("tls config");
        assert_eq!(tls.alpn_protocols, vec![b"h2".to_vec()]);
    }
}
