//! Error type shared by every command.
//!
//! Rule: the `Display`/`Debug` text stays internal (logs only). What crosses the
//! IPC boundary is a `{ code, message }` pair whose message is written for a
//! human, so raw IO/TLS strings never surface in the UI.

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("server `{0}` is not in the catalog")]
    ServerNotFound(String),

    #[error("server `{0}` has no credentials configured")]
    MissingCredentials(String),

    #[error("tunnel is already running")]
    AlreadyConnected,

    #[error("tunnel is not running")]
    NotConnected,

    #[error("could not bind local listener on port {port}: {source}")]
    Bind {
        port: u16,
        #[source]
        source: std::io::Error,
    },

    #[error("outbound dial failed: {0}")]
    Dial(String),

    #[error("tls handshake failed: {0}")]
    Tls(String),

    #[error("remote rejected the session: {0}")]
    Protocol(String),

    #[error("system proxy update failed: {0}")]
    SystemProxy(String),

    #[error("config store failure: {0}")]
    Config(String),

    #[error("invalid input: {0}")]
    Invalid(String),

    #[error("license check failed: {0}")]
    License(String),

    #[error("internal failure: {0}")]
    Internal(String),
}

impl AppError {
    /// Stable machine-readable code, for the UI to branch on.
    pub fn code(&self) -> &'static str {
        match self {
            Self::ServerNotFound(_) => "SERVER_NOT_FOUND",
            Self::MissingCredentials(_) => "MISSING_CREDENTIALS",
            Self::AlreadyConnected => "ALREADY_CONNECTED",
            Self::NotConnected => "NOT_CONNECTED",
            Self::Bind { .. } => "PORT_UNAVAILABLE",
            Self::Dial(_) => "UNREACHABLE",
            Self::Tls(_) => "TLS_FAILED",
            Self::Protocol(_) => "PROTOCOL_REJECTED",
            Self::SystemProxy(_) => "SYSTEM_PROXY_FAILED",
            Self::Config(_) => "CONFIG_FAILED",
            Self::Invalid(_) => "INVALID_INPUT",
            Self::License(_) => "LICENSE_REQUIRED",
            Self::Internal(_) => "INTERNAL",
        }
    }

    /// Text safe to render verbatim in the UI.
    pub fn user_message(&self) -> String {
        match self {
            Self::ServerNotFound(_) => {
                "That location is no longer available. Pick another one.".into()
            }
            Self::MissingCredentials(name) => {
                format!("{name} needs to be set up before you can connect. Add its access key in Settings.")
            }
            Self::AlreadyConnected => "You're already connected.".into(),
            Self::NotConnected => "You're not connected right now.".into(),
            Self::Bind { port, .. } => {
                format!("Local port {port} is already in use. Choose a different port in Settings.")
            }
            Self::Dial(_) => {
                "Couldn't reach that server. Check your internet connection and try again.".into()
            }
            Self::Tls(_) => {
                // Almost always a certificate that matches neither the endpoint
                // nor the masking domain, so name the actual next step.
                "The secure handshake was rejected. The server's certificate doesn't match its address or masking domain - re-add the link, or add allowInsecure=1 if you control the server.".into()
            }
            Self::Protocol(_) => {
                "The server refused the session. Your access key may be wrong or expired.".into()
            }
            Self::SystemProxy(_) => {
                "Couldn't update your system network settings. Try relaunching the app.".into()
            }
            Self::Config(_) => "Couldn't save your settings to disk.".into(),
            Self::Invalid(detail) => detail.clone(),
            // Already written for a person by `core::license`.
            Self::License(detail) => detail.clone(),
            Self::Internal(_) => "Something went wrong. Please try again.".into(),
        }
    }
}

/// Serialised shape seen by the frontend.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        // Full detail goes to the log, never to the webview.
        log::error!("[{}] {self}", self.code());
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.user_message())?;
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
