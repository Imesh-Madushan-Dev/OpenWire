//! Sockets the app opens itself: to the server, and direct.
//!
//! While system-wide routing is on, the routing table sends nearly
//! everything into the adapter - which leads back into this app's own proxy.
//! The app's own connections must not follow it there, or the tunnel would
//! try to carry itself and every connection would loop until something ran
//! out. Two guards, so neither has to be perfect:
//!
//! - **Pinned server addresses.** The server's name is resolved once, before
//!   routing starts. The service cuts exactly those addresses out of the
//!   adapter's routes, and dials use them instead of resolving again - a
//!   lookup now would itself go through the tunnel it is trying to reach.
//! - **Bound to the real network.** Each socket is tied to the interface of
//!   the machine's own default route (`IP_UNICAST_IF`), so even an address
//!   the routes know nothing about leaves on the real network. WireGuard
//!   binds its socket the same way.
//!
//! With routing off, [`connect`] is exactly `TcpStream::connect`.

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::RwLock;

use tokio::net::{TcpStream, UdpSocket};

struct Bypass {
    /// The adapter's interface index, once it exists.
    adapter: Option<u32>,
    /// Host name (lowercase) to the addresses it was pinned to.
    pins: HashMap<String, Vec<IpAddr>>,
}

static BYPASS: RwLock<Option<Bypass>> = RwLock::new(None);

/// Pins host names to addresses for the session ahead. Called before routing
/// starts, so no dial in between resolves through the adapter.
pub fn pin(pins: HashMap<String, Vec<IpAddr>>) {
    let pins = pins
        .into_iter()
        .map(|(host, addresses)| (host.to_ascii_lowercase(), addresses))
        .collect();
    if let Ok(mut bypass) = BYPASS.write() {
        *bypass = Some(Bypass {
            adapter: None,
            pins,
        });
    }
}

/// Starts binding sockets away from the adapter.
pub fn avoid(adapter: u32) {
    if let Ok(mut bypass) = BYPASS.write() {
        if let Some(bypass) = bypass.as_mut() {
            bypass.adapter = Some(adapter);
        }
    }
}

/// Back to plain sockets, when routing ends.
pub fn release() {
    if let Ok(mut bypass) = BYPASS.write() {
        *bypass = None;
    }
}

fn adapter() -> Option<u32> {
    BYPASS.read().ok()?.as_ref()?.adapter
}

fn pinned(host: &str) -> Option<Vec<IpAddr>> {
    let bypass = BYPASS.read().ok()?;
    bypass
        .as_ref()?
        .pins
        .get(&host.trim_end_matches('.').to_ascii_lowercase())
        .cloned()
}

/// Every address `host` resolves to, pins first.
pub async fn resolve(host: &str, port: u16) -> io::Result<Vec<SocketAddr>> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    if let Some(addresses) = pinned(host) {
        return Ok(addresses
            .into_iter()
            .map(|ip| SocketAddr::new(ip, port))
            .collect());
    }
    Ok(tokio::net::lookup_host((host, port)).await?.collect())
}

/// Resolves a server's name for pinning, before routing starts.
pub async fn lookup(host: &str) -> io::Result<Vec<IpAddr>> {
    let mut addresses: Vec<IpAddr> = resolve(host, 0)
        .await?
        .into_iter()
        .map(|a| a.ip())
        .collect();
    addresses.sort();
    addresses.dedup();
    if addresses.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{host} has no address"),
        ));
    }
    Ok(addresses)
}

/// A TCP connection to `host:port`, trying each address in turn.
pub async fn connect(host: &str, port: u16) -> io::Result<TcpStream> {
    let mut last = io::Error::new(io::ErrorKind::NotFound, format!("{host} has no address"));
    for address in resolve(host, port).await? {
        match connect_to(address).await {
            Ok(stream) => return Ok(stream),
            Err(err) => last = err,
        }
    }
    Err(last)
}

async fn connect_to(address: SocketAddr) -> io::Result<TcpStream> {
    let Some(adapter) = adapter() else {
        return TcpStream::connect(address).await;
    };
    let socket = if address.is_ipv4() {
        tokio::net::TcpSocket::new_v4()?
    } else {
        tokio::net::TcpSocket::new_v6()?
    };
    egress::bind(&socket, address.is_ipv4(), adapter);
    socket.connect(address).await
}

/// A UDP socket connected to `target`, for a direct flow.
pub async fn udp_to(target: SocketAddr) -> io::Result<UdpSocket> {
    let any: SocketAddr = if target.is_ipv4() {
        ([0, 0, 0, 0], 0).into()
    } else {
        ([0u16; 8], 0).into()
    };
    let socket = UdpSocket::bind(any).await?;
    if let Some(adapter) = adapter() {
        egress::bind(&socket, target.is_ipv4(), adapter);
    }
    socket.connect(target).await?;
    Ok(socket)
}

#[cfg(windows)]
mod egress {
    use std::os::windows::io::AsRawSocket;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use windows_sys::Win32::NetworkManagement::IpHelper::{
        FreeMibTable, GetIpForwardTable2, GetIpInterfaceEntry, InitializeIpInterfaceEntry,
        MIB_IPFORWARD_ROW2, MIB_IPFORWARD_TABLE2, MIB_IPINTERFACE_ROW,
    };
    use windows_sys::Win32::Networking::WinSock::{
        setsockopt, ADDRESS_FAMILY, AF_INET, AF_INET6, IPPROTO_IP, IPPROTO_IPV6, IPV6_UNICAST_IF,
        IP_UNICAST_IF, SOCKET,
    };

    /// Routes change on roaming, not per connection; reading the table for
    /// every dial would be waste.
    const FRESH_FOR: Duration = Duration::from_secs(2);

    /// When it was read, for which adapter, and what it found.
    type Reading = (Instant, u32, Option<u32>);
    /// One reading per family: IPv4, then IPv6.
    static CACHE: Mutex<[Option<Reading>; 2]> = Mutex::new([None, None]);

    /// Ties `socket` to the real network's interface for its family.
    pub fn bind(socket: &impl AsRawSocket, ipv4: bool, adapter: u32) {
        let Some(index) = default_interface(ipv4, adapter) else {
            // No other default route: nothing to bind to, and nothing a
            // binding would protect - the pinned routes still apply.
            return;
        };
        let raw = socket.as_raw_socket() as SOCKET;
        let (level, option, value) = if ipv4 {
            // IPv4 wants the index in network byte order; IPv6 does not.
            (IPPROTO_IP, IP_UNICAST_IF, index.to_be())
        } else {
            (IPPROTO_IPV6, IPV6_UNICAST_IF, index)
        };
        // SAFETY: a live socket, and a u32 option value of the size given.
        let result = unsafe {
            setsockopt(
                raw,
                level,
                option,
                (&value as *const u32).cast(),
                std::mem::size_of::<u32>() as i32,
            )
        };
        if result != 0 {
            log::warn!("could not bind a socket to interface {index}");
        }
    }

    /// The interface behind the lowest-cost default route that is not the
    /// adapter's.
    fn default_interface(ipv4: bool, adapter: u32) -> Option<u32> {
        let slot = usize::from(!ipv4);
        if let Ok(cache) = CACHE.lock() {
            if let Some((at, cached_adapter, index)) = cache[slot] {
                if cached_adapter == adapter && at.elapsed() < FRESH_FOR {
                    return index;
                }
            }
        }
        let index = read_default_interface(if ipv4 { AF_INET } else { AF_INET6 }, adapter);
        if let Ok(mut cache) = CACHE.lock() {
            cache[slot] = Some((Instant::now(), adapter, index));
        }
        index
    }

    fn read_default_interface(family: ADDRESS_FAMILY, adapter: u32) -> Option<u32> {
        let mut table: *mut MIB_IPFORWARD_TABLE2 = std::ptr::null_mut();
        // SAFETY: on success the OS hands back a table it owns, freed below.
        if unsafe { GetIpForwardTable2(family, &mut table) } != 0 || table.is_null() {
            return None;
        }
        // SAFETY: `NumEntries` rows follow the header contiguously.
        let rows = unsafe {
            std::slice::from_raw_parts(
                std::ptr::addr_of!((*table).Table).cast::<MIB_IPFORWARD_ROW2>(),
                (*table).NumEntries as usize,
            )
        };
        let best = rows
            .iter()
            .filter(|row| row.DestinationPrefix.PrefixLength == 0 && row.InterfaceIndex != adapter)
            .filter_map(|row| {
                interface_metric(family, row.InterfaceIndex)
                    .map(|metric| (row.Metric.saturating_add(metric), row.InterfaceIndex))
            })
            .min()
            .map(|(_, index)| index);
        unsafe { FreeMibTable(table.cast()) };
        best
    }

    /// `None` for an interface that is down for this family.
    fn interface_metric(family: ADDRESS_FAMILY, index: u32) -> Option<u32> {
        let mut row = MIB_IPINTERFACE_ROW::default();
        // SAFETY: initialises the row in place.
        unsafe { InitializeIpInterfaceEntry(&mut row) };
        row.Family = family;
        row.InterfaceIndex = index;
        // SAFETY: family and index identify the row to read.
        if unsafe { GetIpInterfaceEntry(&mut row) } != 0 || !row.Connected {
            return None;
        }
        Some(row.Metric)
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn this_machine_has_a_default_route_somewhere() {
            // A build machine is online; the index is whatever it is, but
            // asking must not hang or panic, and must name a real interface.
            if let Some(index) = super::default_interface(true, u32::MAX) {
                assert_ne!(index, 0);
            }
        }
    }
}

#[cfg(not(windows))]
mod egress {
    pub fn bind<S>(_socket: &S, _ipv4: bool, _adapter: u32) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One test, because the pins are process-wide.
    #[tokio::test]
    async fn pins_answer_before_the_resolver_and_go_away_on_release() {
        let host = "pinned.openwire.invalid";
        assert!(
            resolve(host, 443).await.is_err(),
            "unresolvable without a pin"
        );

        pin(HashMap::from([(
            host.to_string(),
            vec!["203.0.113.10".parse().unwrap()],
        )]));
        assert_eq!(
            resolve("Pinned.OpenWire.invalid.", 443).await.unwrap(),
            vec!["203.0.113.10:443".parse::<SocketAddr>().unwrap()],
            "case and a trailing dot do not matter"
        );
        assert_eq!(
            resolve("198.51.100.1", 80).await.unwrap(),
            vec!["198.51.100.1:80".parse::<SocketAddr>().unwrap()],
            "literals never touch the pins"
        );

        release();
        assert!(resolve(host, 443).await.is_err());
    }
}
