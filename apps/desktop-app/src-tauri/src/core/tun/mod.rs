//! System-wide routing: a virtual network adapter that carries every app's
//! traffic into the tunnel, not only the apps that follow the system proxy.
//!
//! Creating an adapter, changing routes and installing firewall rules all
//! need administrator rights, so they live in a small Windows service - the
//! same split WireGuard, Mullvad and Tailscale use. The app stays unelevated
//! and keeps everything else: the server, its credentials, the protocols,
//! usage accounting and the kill switch's health checks.
//!
//! ```text
//!  every app ─▶ Wintun adapter ─▶ OpenWire Tunnel service ─▶ app's local proxy ─▶ server
//!               (routes, DNS)     (userspace TCP/IP stack)    (127.0.0.1, VLESS/Trojan)
//! ```
//!
//! The service turns the packets apps send into TCP streams and UDP flows
//! with a userspace TCP/IP stack, and hands each one to the app's own local
//! proxy over loopback - see [`wire`]. It never sees a server address beyond
//! the ones it must route around, nor any credential.
//!
//! Lifetime is tied to the control pipe: the session lasts exactly as long as
//! the app keeps its pipe open. If the app exits or crashes, the service
//! takes the adapter, its routes and its firewall rules down at once, so the
//! machine is never left pointing at a proxy that is no longer there. If the
//! service itself dies, Windows removes the adapter (and with it the routes)
//! and the firewall rules, which live in a dynamic session.
//!
//! One binary plays every role, chosen by a flag before Tauri starts:
//!
//! - `--tunnel-service` - the service, launched by the SCM;
//! - `--tunnel-install` / `--tunnel-uninstall` - the elevated helper that
//!   registers or removes it.

pub mod cidr;
pub mod dns;
pub mod engine;
pub mod upstream;
pub mod wire;

#[cfg(windows)]
pub mod client;
#[cfg(windows)]
mod firewall;
#[cfg(windows)]
mod netcfg;
#[cfg(windows)]
pub mod scm;
#[cfg(windows)]
mod service;
#[cfg(windows)]
mod verify;
#[cfg(windows)]
mod wintun;

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize};

pub const SERVICE_NAME: &str = "OpenWireTunnel";
pub const SERVICE_DISPLAY_NAME: &str = "OpenWire Tunnel";
pub const PIPE_NAME: &str = r"\\.\pipe\openwire-tunnel";

/// Bumped whenever the control protocol changes incompatibly. The service is
/// a copy taken at install time, so after an app update the two can disagree;
/// the app then asks for the service to be updated rather than misread it.
pub const PROTOCOL: u32 = 1;

pub const FLAG_SERVICE: &str = "--tunnel-service";
pub const FLAG_INSTALL: &str = "--tunnel-install";
pub const FLAG_UNINSTALL: &str = "--tunnel-uninstall";

/// The adapter's name in Network Connections.
pub const ADAPTER_NAME: &str = "OpenWire";

/// The adapter's own address. A /30 from the 172.16/12 private block, the
/// range sing-box uses for the same job, so it rarely meets a real network.
pub const INTERFACE_V4: Ipv4Addr = Ipv4Addr::new(172, 19, 0, 1);
pub const PREFIX_V4: u8 = 30;
/// The DNS server Windows is told to use. Nothing answers here: the service
/// catches every query on the adapter and resolves it through the tunnel.
pub const DNS_V4: Ipv4Addr = Ipv4Addr::new(172, 19, 0, 2);
/// A unique local address. Windows prefers IPv4 over it, so IPv6 only
/// carries what has no IPv4 address at all, and no IPv6 traffic leaks past
/// the adapter.
pub const INTERFACE_V6: Ipv6Addr = Ipv6Addr::new(0xfdfe, 0xdcba, 0x9876, 0, 0, 0, 0, 1);
pub const PREFIX_V6: u8 = 126;

/// Packets never cross a real link here - the stack ends on this machine -
/// so this is only what Windows segments to, not a path constraint.
pub const MTU: u16 = 1500;

/// Whether `ip` is in the adapter's own subnets. Traffic to or from one is
/// system-wide routing's, which the app's relay already counts against the
/// apps behind it - so the network monitor leaves it alone.
pub fn on_adapter(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let mask = u32::MAX << (32 - PREFIX_V4);
            u32::from(*v4) & mask == u32::from(INTERFACE_V4) & mask
        }
        IpAddr::V6(v6) => {
            let mask = u128::MAX << (128 - PREFIX_V6);
            u128::from(*v6) & mask == u128::from(INTERFACE_V6) & mask
        }
    }
}

/// Resolvers the service sends queries to, through the tunnel, when the app
/// names none. Reached over TCP, which every server carries.
pub const DEFAULT_DNS: [IpAddr; 2] = [
    IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
    IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
];

/// Caps on what a client may ask for, so a request cannot make the service
/// build an unbounded route table.
pub const MAX_EXCLUDED: usize = 64;
pub const MAX_DNS: usize = 4;

/// One line from the app to the service, as JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Request {
    Start(StartRequest),
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRequest {
    /// The app's local proxy on 127.0.0.1. The service checks the app
    /// really owns it before sending anything there.
    pub proxy_port: u16,
    /// Addresses that must not enter the adapter: the server's own, or the
    /// tunnel would carry itself.
    pub exclude: Vec<IpAddr>,
    /// Keep private address space off the adapter.
    pub lan_bypass: bool,
    /// Block anything that tries to leave outside the adapter.
    pub kill_switch: bool,
    /// Resolvers to query through the tunnel; empty for [`DEFAULT_DNS`].
    pub dns: Vec<IpAddr>,
}

/// One line from the service to the app, as JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Event {
    Hello {
        protocol: u32,
        version: String,
    },
    /// The adapter is up and routing.
    Started {
        /// The adapter's interface index, which the app's own sockets avoid.
        interface_index: u32,
        /// Whether IPv6 is routed too. When it could not be set up, the
        /// kill switch (if on) still blocks it.
        ipv6: bool,
    },
    /// The session ended. `reason` is set when it was not asked to, in words
    /// fit for the UI.
    Stopped {
        reason: Option<String>,
    },
    /// A request failed, in words fit for the UI.
    Error {
        message: String,
    },
}

/// Runs the role a command-line flag asks for. `None` means no flag matched.
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
        // ponytail: macOS (utun via a NetworkExtension) and Linux (/dev/net/tun
        // with a polkit helper) would plug in here, reusing `engine` as is.
        let _ = first;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_events_use_the_tagged_shape() {
        let start = Request::Start(StartRequest {
            proxy_port: 10808,
            exclude: vec!["203.0.113.10".parse().unwrap()],
            lan_bypass: true,
            kill_switch: false,
            dns: vec![],
        });
        let line = serde_json::to_string(&start).unwrap();
        assert!(
            line.starts_with(r#"{"type":"start","proxyPort":10808"#),
            "{line}"
        );
        assert_eq!(serde_json::from_str::<Request>(&line).unwrap(), start);

        assert_eq!(
            serde_json::to_string(&Request::Stop).unwrap(),
            r#"{"type":"stop"}"#
        );
        let started: Event =
            serde_json::from_str(r#"{"type":"started","interfaceIndex":7,"ipv6":true}"#).unwrap();
        assert_eq!(
            started,
            Event::Started {
                interface_index: 7,
                ipv6: true
            }
        );
    }

    #[test]
    fn the_adapter_subnets_are_recognised_and_nothing_else_is() {
        for on in ["172.19.0.1", "172.19.0.2", "172.19.0.3", "fdfe:dcba:9876::1"] {
            assert!(on_adapter(&on.parse().unwrap()), "{on}");
        }
        for off in ["172.19.0.4", "172.19.1.1", "192.168.1.10", "fdfe:dcba:9876::5", "::1"] {
            assert!(!on_adapter(&off.parse().unwrap()), "{off}");
        }
    }

    #[test]
    fn the_dns_address_sits_inside_the_adapter_subnet() {
        let mask = u32::MAX << (32 - PREFIX_V4);
        assert_eq!(
            u32::from(INTERFACE_V4) & mask,
            u32::from(DNS_V4) & mask,
            "Windows only sends to the DNS address through the adapter if it is on-link"
        );
        assert_ne!(INTERFACE_V4, DNS_V4);
    }
}
