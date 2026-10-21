//! Device-wide network monitor.
//!
//! Without elevation Windows will say how many bytes each *app* moved, but
//! never where to - which is why the Hosts and Traffic-type columns used to
//! cover only what passed through Maizy. The monitor closes that gap without a
//! driver: a small service, installed once with the user's admin approval,
//! reads the kernel's own per-packet network events through ETW, names each
//! remote address from the DNS client's lookups, and streams per-second totals
//! by app, host and traffic type to the app over a local named pipe.
//!
//! One binary plays every role, chosen by a flag before Tauri starts:
//!
//! - `--netmon-service` - the service itself, launched by the SCM;
//! - `--netmon-install` / `--netmon-uninstall` - the elevated helper the app
//!   launches through UAC to register or remove it.
//!
//! The service runs from its own copy of the binary under Program Files. The
//! app is installed per user, in a folder the user can write to, and a SYSTEM
//! service started from there would let anything running as the user replace
//! it and inherit SYSTEM. The pipe is one-way, service to app: the service
//! never parses a byte a client sends.

pub mod client;
pub mod dns;
pub mod flows;
pub mod parse;

#[cfg(windows)]
mod etw;
#[cfg(windows)]
pub mod scm;
#[cfg(windows)]
mod service;
#[cfg(windows)]
mod sys;

use serde::{Deserialize, Serialize};

pub const SERVICE_NAME: &str = "MaizyNetMon";
pub const SERVICE_DISPLAY_NAME: &str = "Maizy VPN Network Monitor";
pub const PIPE_NAME: &str = r"\\.\pipe\maizy-netmon";

/// Bumped whenever the wire format changes incompatibly. The service is a
/// copy taken at install time, so after an app update the two can disagree;
/// the app then asks to reinstall rather than misreading the stream.
pub const PROTOCOL: u32 = 1;

/// Bumped whenever the service starts sending something the app shows, in a
/// shape older apps can ignore. A service with a lower revision still speaks
/// the protocol and is read as before, but is reported as outdated - without
/// that, what it lacks would simply never appear, with nothing to say why.
///
/// 1: hosts carry the country their address is in.
pub const REVISION: u32 = 1;

pub const FLAG_SERVICE: &str = "--netmon-service";
pub const FLAG_INSTALL: &str = "--netmon-install";
pub const FLAG_UNINSTALL: &str = "--netmon-uninstall";
/// Passed to the service: the app executable it was installed from, so it can
/// remove itself once the app has been uninstalled.
pub const FLAG_OWNER: &str = "--owner";

/// One line on the pipe, as JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Message {
    Hello {
        protocol: u32,
        version: String,
        /// See [`REVISION`]. Absent from services that predate it, which are
        /// revision 0.
        #[serde(default)]
        revision: u32,
    },
    /// Whether the trace session is up. `error` says why not, in words fit
    /// for the settings page.
    Status {
        collecting: bool,
        error: Option<String>,
    },
    Usage {
        rows: Vec<Row>,
    },
}

/// Traffic one app exchanged with one host, of one kind, around time `t`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    /// Unix seconds. The app buckets it into its own local day, so the
    /// service never needs an opinion about timezones.
    pub t: i64,
    pub app: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub host: String,
    /// ISO code of the country the host's address is in, looked up here
    /// while the address is still known - the row names the host, not the IP.
    /// Optional on the wire, so a service installed before it existed still
    /// speaks the same protocol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    pub kind: String,
    pub up: u64,
    pub down: u64,
}

/// Runs the role a command-line flag asks for. `None` means no flag matched
/// and this process is the app.
pub fn dispatch() -> Option<i32> {
    let first = std::env::args().nth(1)?;

    #[cfg(windows)]
    {
        match first.as_str() {
            FLAG_SERVICE => Some(service::run()),
            FLAG_INSTALL => Some(scm::install_from_cli()),
            FLAG_UNINSTALL => Some(scm::uninstall_from_cli()),
            _ => None,
        }
    }

    #[cfg(not(windows))]
    {
        // ponytail: macOS has no ETW; the equivalent is a privileged helper
        // reading `nettop`/NetworkStatistics, if the app ever ships there.
        let _ = first;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_in_the_tagged_shape_the_client_reads() {
        let message = Message::Usage {
            rows: vec![Row {
                t: 1,
                app: "a.exe".into(),
                path: None,
                host: "h".into(),
                country: None,
                kind: "HTTPS".into(),
                up: 2,
                down: 3,
            }],
        };
        let line = serde_json::to_string(&message).unwrap();
        assert!(line.starts_with(r#"{"type":"usage""#), "{line}");
        assert!(
            !line.contains("path") && !line.contains("country"),
            "absent fields cost nothing on the wire"
        );
        assert_eq!(serde_json::from_str::<Message>(&line).unwrap(), message);

        let status = serde_json::to_string(&Message::Status {
            collecting: false,
            error: Some("x".into()),
        })
        .unwrap();
        assert_eq!(
            status,
            r#"{"type":"status","collecting":false,"error":"x"}"#
        );
    }

    #[test]
    fn a_service_from_before_revisions_reads_as_revision_zero() {
        let hello: Message =
            serde_json::from_str(r#"{"type":"hello","protocol":1,"version":"0.1.0"}"#).unwrap();
        assert_eq!(
            hello,
            Message::Hello {
                protocol: 1,
                version: "0.1.0".into(),
                revision: 0,
            }
        );
    }
}
