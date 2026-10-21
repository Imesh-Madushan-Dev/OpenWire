//! The operating system's view of the local network.
//!
//! All of it is plain IP Helper calls that need no elevation: which adapter
//! is the LAN, what the ARP cache already knows, and an ARP request to one
//! address at a time.

use std::net::Ipv4Addr;

/// The adapter a scan runs on: the physical one carrying the default route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adapter {
    pub index: u32,
    /// What Windows calls it, e.g. "Wi-Fi".
    pub name: String,
    pub wireless: bool,
    pub mac: Option<[u8; 6]>,
    pub ip: Ipv4Addr,
    pub prefix: u8,
    pub gateway: Ipv4Addr,
    pub dns: Vec<Ipv4Addr>,
}

/// An entry from the ARP cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Neighbor {
    pub ip: Ipv4Addr,
    pub mac: [u8; 6],
    /// Confirmed within the last half-minute or so, rather than remembered.
    pub reachable: bool,
}

/// Unicast, assigned, and not all zeros: an address a real device answers to.
pub fn is_device_mac(mac: [u8; 6]) -> bool {
    mac[0] & 0x01 == 0 && mac != [0; 6]
}

#[cfg(windows)]
pub use windows::{arp, neighbors, primary_adapter};

#[cfg(not(windows))]
pub fn primary_adapter() -> Option<Adapter> {
    // ponytail: getifaddrs + the routing table on macOS/Linux, if the app
    // ever ships there. Without it the Devices view says it needs Windows.
    None
}

#[cfg(not(windows))]
pub fn neighbors(_index: u32) -> Vec<Neighbor> {
    Vec::new()
}

#[cfg(not(windows))]
pub fn arp(_target: Ipv4Addr, _source: Ipv4Addr) -> Option<[u8; 6]> {
    None
}

#[cfg(windows)]
mod windows {
    use std::net::Ipv4Addr;

    use windows_sys::core::PWSTR;
    use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, NO_ERROR};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        FreeMibTable, GetAdaptersAddresses, GetIpNetTable2, SendARP, GAA_FLAG_INCLUDE_GATEWAYS,
        GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_MULTICAST, IF_TYPE_ETHERNET_CSMACD,
        IF_TYPE_IEEE80211, IP_ADAPTER_ADDRESSES_LH, MIB_IPNET_ROW2, MIB_IPNET_TABLE2,
    };
    use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    use windows_sys::Win32::Networking::WinSock::{
        NlnsIncomplete, NlnsReachable, NlnsUnreachable, AF_INET, SOCKADDR_IN, SOCKET_ADDRESS,
    };

    use super::{is_device_mac, Adapter, Neighbor};

    /// Words that mark an adapter as virtual - VPN tunnels, hypervisor
    /// switches, packet-capture shims - even when it claims to be Ethernet.
    const VIRTUAL: &[&str] = &[
        "virtual", "vpn", "tap-", "tap adapter", "wintun", "wireguard", "tunnel", "hyper-v",
        "vethernet", "vmware", "virtualbox", "loopback", "npcap", "tailscale", "zerotier",
        "openvpn", "pseudo",
    ];

    /// Picks the LAN: an up, physical Ethernet or Wi-Fi adapter with an IPv4
    /// address and a gateway on its own subnet. With several, the one Windows
    /// would route through first.
    pub fn primary_adapter() -> Option<Adapter> {
        let flags = GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST;
        let mut size: u32 = 16 * 1024;

        for _ in 0..3 {
            // u64s, so the buffer meets the structures' 8-byte alignment.
            let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
            let head = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
            // SAFETY: `size` is the buffer's length in bytes; on overflow the
            // OS writes the size it needs and the call is retried.
            let result = unsafe {
                GetAdaptersAddresses(u32::from(AF_INET), flags, std::ptr::null(), head, &mut size)
            };
            if result == ERROR_BUFFER_OVERFLOW {
                continue;
            }
            if result != NO_ERROR {
                return None;
            }

            let mut best: Option<(u32, Adapter)> = None;
            let mut node = head;
            // SAFETY: a linked list the OS built inside `buffer`, which
            // outlives this loop; every pointer read is null-checked.
            unsafe {
                while !node.is_null() {
                    let adapter = &*node;
                    node = adapter.Next;
                    if let Some(candidate) = read_adapter(adapter) {
                        let metric = adapter.Ipv4Metric;
                        if best.as_ref().map_or(true, |(lowest, _)| metric < *lowest) {
                            best = Some((metric, candidate));
                        }
                    }
                }
            }
            return best.map(|(_, adapter)| adapter);
        }
        None
    }

    unsafe fn read_adapter(adapter: &IP_ADAPTER_ADDRESSES_LH) -> Option<Adapter> {
        let wireless = adapter.IfType == IF_TYPE_IEEE80211;
        if adapter.OperStatus != IfOperStatusUp
            || !(wireless || adapter.IfType == IF_TYPE_ETHERNET_CSMACD)
        {
            return None;
        }

        let name = wide(adapter.FriendlyName);
        let description = wide(adapter.Description);
        let label = format!("{name} {description}").to_ascii_lowercase();
        if VIRTUAL.iter().any(|word| label.contains(word)) {
            return None;
        }

        // The first IPv4 address with a sane LAN-sized prefix.
        let mut ip = None;
        let mut unicast = adapter.FirstUnicastAddress;
        while !unicast.is_null() {
            let entry = &*unicast;
            unicast = entry.Next;
            if let Some(address) = ipv4(&entry.Address) {
                if (8..=30).contains(&entry.OnLinkPrefixLength) && !address.is_link_local() {
                    ip = Some((address, entry.OnLinkPrefixLength));
                    break;
                }
            }
        }
        let (ip, prefix) = ip?;

        let mask = u32::MAX << (32 - u32::from(prefix));
        let same_subnet = |other: Ipv4Addr| u32::from(other) & mask == u32::from(ip) & mask;

        let mut gateway = None;
        let mut entry = adapter.FirstGatewayAddress;
        while !entry.is_null() {
            let item = &*entry;
            entry = item.Next;
            if let Some(address) = ipv4(&item.Address).filter(|a| same_subnet(*a) && !a.is_unspecified()) {
                gateway = Some(address);
                break;
            }
        }
        let gateway = gateway?;

        let mut dns = Vec::new();
        let mut server = adapter.FirstDnsServerAddress;
        while !server.is_null() {
            let item = &*server;
            server = item.Next;
            if let Some(address) = ipv4(&item.Address) {
                dns.push(address);
            }
        }

        let mac = (adapter.PhysicalAddressLength == 6).then(|| {
            let mut mac = [0u8; 6];
            mac.copy_from_slice(&adapter.PhysicalAddress[..6]);
            mac
        });

        Some(Adapter {
            index: adapter.Anonymous1.Anonymous.IfIndex,
            name,
            wireless,
            mac,
            ip,
            prefix,
            gateway,
            dns,
        })
    }

    /// The ARP cache for one interface.
    pub fn neighbors(index: u32) -> Vec<Neighbor> {
        let mut table: *mut MIB_IPNET_TABLE2 = std::ptr::null_mut();
        // SAFETY: on success the OS hands back a table it owns, released below.
        if unsafe { GetIpNetTable2(AF_INET, &mut table) } != NO_ERROR || table.is_null() {
            return Vec::new();
        }

        let mut out = Vec::new();
        // SAFETY: `NumEntries` rows follow the header contiguously.
        unsafe {
            let rows = std::slice::from_raw_parts(
                std::ptr::addr_of!((*table).Table).cast::<MIB_IPNET_ROW2>(),
                (*table).NumEntries as usize,
            );
            for row in rows {
                if row.InterfaceIndex != index
                    || row.PhysicalAddressLength != 6
                    || row.State == NlnsUnreachable
                    || row.State == NlnsIncomplete
                    || row.Address.si_family != AF_INET
                {
                    continue;
                }
                let mut mac = [0u8; 6];
                mac.copy_from_slice(&row.PhysicalAddress[..6]);
                if !is_device_mac(mac) {
                    continue;
                }
                let raw = row.Address.Ipv4.sin_addr.S_un.S_addr;
                out.push(Neighbor {
                    ip: Ipv4Addr::from(raw.to_ne_bytes()),
                    mac,
                    reachable: row.State == NlnsReachable,
                });
            }
            FreeMibTable(table.cast());
        }
        out
    }

    /// Asks `target` for its hardware address. Blocks until it answers or
    /// Windows gives up - a few seconds for an address nobody holds - so it
    /// belongs on a blocking thread.
    pub fn arp(target: Ipv4Addr, source: Ipv4Addr) -> Option<[u8; 6]> {
        let mut mac = [0u8; 8];
        let mut length: u32 = mac.len() as u32;
        // SAFETY: `mac` is `length` bytes long, and both addresses are in
        // network byte order, which is their octets read natively.
        let result = unsafe {
            SendARP(
                u32::from_ne_bytes(target.octets()),
                u32::from_ne_bytes(source.octets()),
                mac.as_mut_ptr().cast(),
                &mut length,
            )
        };
        if result != NO_ERROR || length != 6 {
            return None;
        }
        let mut out = [0u8; 6];
        out.copy_from_slice(&mac[..6]);
        is_device_mac(out).then_some(out)
    }

    unsafe fn ipv4(address: &SOCKET_ADDRESS) -> Option<Ipv4Addr> {
        if address.lpSockaddr.is_null() || (*address.lpSockaddr).sa_family != AF_INET {
            return None;
        }
        let sin = &*address.lpSockaddr.cast::<SOCKADDR_IN>();
        Some(Ipv4Addr::from(sin.sin_addr.S_un.S_addr.to_ne_bytes()))
    }

    unsafe fn wide(text: PWSTR) -> String {
        if text.is_null() {
            return String::new();
        }
        let mut len = 0;
        while *text.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(text, len))
    }
}
