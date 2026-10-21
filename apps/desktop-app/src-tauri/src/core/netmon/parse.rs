//! Decoders for the two ETW events the monitor consumes.
//!
//! Pure functions over the raw payload, so they are exercised without a live
//! trace session - which needs admin rights no test run has. The layouts are
//! the providers' manifest templates (`Get-WinEvent -ListProvider ...` prints
//! them); they have not changed since Windows 8.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// `Microsoft-Windows-Kernel-Network`: TCP and UDP sends and receives.
pub const KERNEL_NETWORK_EVENTS: [u16; 8] = [10, 11, 26, 27, 42, 43, 58, 59];
/// Its IPv4 and IPv6 keywords.
pub const KERNEL_NETWORK_KEYWORDS: u64 = 0x10 | 0x20;
/// `Microsoft-Windows-DNS-Client`: a query completed.
pub const DNS_QUERY_COMPLETED: u16 = 3008;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Proto {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Send,
    Recv,
}

/// One send or receive, exactly as the kernel reported it.
///
/// `d*` and `s*` are kept as the event names them rather than resolved into
/// local and remote here: which side is ours is a question about this
/// machine's addresses, answered once a second by the aggregator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Packet {
    pub pid: u32,
    pub size: u32,
    pub proto: Proto,
    pub direction: Direction,
    pub daddr: IpAddr,
    pub dport: u16,
    pub saddr: IpAddr,
    pub sport: u16,
}

/// Decodes a Kernel-Network event, or `None` for an id it does not carry
/// traffic in, or a payload too short to be what it claims.
///
/// Template, all versions: `PID u32, size u32, daddr, saddr, dport u16,
/// sport u16, ...` - addresses are 4 bytes for IPv4 events and 16 for IPv6,
/// and both they and the ports are in network byte order.
pub fn network_event(id: u16, data: &[u8]) -> Option<Packet> {
    let (proto, direction, v6) = match id {
        10 => (Proto::Tcp, Direction::Send, false),
        11 => (Proto::Tcp, Direction::Recv, false),
        26 => (Proto::Tcp, Direction::Send, true),
        27 => (Proto::Tcp, Direction::Recv, true),
        42 => (Proto::Udp, Direction::Send, false),
        43 => (Proto::Udp, Direction::Recv, false),
        58 => (Proto::Udp, Direction::Send, true),
        59 => (Proto::Udp, Direction::Recv, true),
        _ => return None,
    };

    let pid = u32_le(data, 0)?;
    let size = u32_le(data, 4)?;

    let (daddr, saddr, ports_at) = if v6 {
        let d: [u8; 16] = data.get(8..24)?.try_into().ok()?;
        let s: [u8; 16] = data.get(24..40)?.try_into().ok()?;
        (
            IpAddr::V6(Ipv6Addr::from(d)).to_canonical(),
            IpAddr::V6(Ipv6Addr::from(s)).to_canonical(),
            40,
        )
    } else {
        let d: [u8; 4] = data.get(8..12)?.try_into().ok()?;
        let s: [u8; 4] = data.get(12..16)?.try_into().ok()?;
        (
            IpAddr::V4(Ipv4Addr::from(d)),
            IpAddr::V4(Ipv4Addr::from(s)),
            16,
        )
    };

    Some(Packet {
        pid,
        size,
        proto,
        direction,
        daddr,
        dport: u16_be(data, ports_at)?,
        saddr,
        sport: u16_be(data, ports_at + 2)?,
    })
}

/// Decodes a DNS-Client "query completed" event into the name that was asked
/// for and every address it resolved to. `None` for a failed query.
///
/// Template: `QueryName wstr, QueryType u32, QueryOptions u64, QueryStatus
/// u32, QueryResults wstr`.
pub fn dns_event(data: &[u8]) -> Option<(String, Vec<IpAddr>)> {
    let (name, after_name) = utf16z(data, 0)?;
    let status = u32_le(data, after_name + 4 + 8)?;
    if status != 0 {
        return None;
    }
    let (results, _) = utf16z(data, after_name + 4 + 8 + 4)?;

    let name = name.trim_end_matches('.').to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }

    let addresses = query_results(&results);
    (!addresses.is_empty()).then_some((name, addresses))
}

/// `QueryResults` is a `;`-separated list mixing addresses - IPv4 often
/// written as v4-mapped IPv6 - with `type: N value` records for CNAMEs and
/// the like, which carry no address and are skipped.
fn query_results(results: &str) -> Vec<IpAddr> {
    results
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty() && !entry.starts_with("type:"))
        .filter_map(|entry| entry.parse::<IpAddr>().ok())
        .map(|address| address.to_canonical())
        .collect()
}

fn u32_le(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

fn u16_be(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

/// A NUL-terminated UTF-16LE string starting at `at`, and the offset just
/// past its terminator.
fn utf16z(data: &[u8], at: usize) -> Option<(String, usize)> {
    let tail = data.get(at..)?;
    let units: Vec<u16> = tail
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();

    // No terminator inside the payload means it was cut short.
    let end = at + (units.len() + 1) * 2;
    if end > data.len() {
        return None;
    }
    Some((String::from_utf16_lossy(&units), end))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4_payload(pid: u32, size: u32, d: [u8; 4], s: [u8; 4], dport: u16, sport: u16) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend(pid.to_le_bytes());
        data.extend(size.to_le_bytes());
        data.extend(d);
        data.extend(s);
        data.extend(dport.to_be_bytes());
        data.extend(sport.to_be_bytes());
        // seqnum + connid, which the decoder does not need but real events carry.
        data.extend([0u8; 8]);
        data
    }

    #[test]
    fn a_tcp_v4_send_decodes_with_ports_in_network_order() {
        let data = v4_payload(
            1234,
            1400,
            [93, 184, 216, 34],
            [192, 168, 1, 20],
            443,
            51000,
        );
        let packet = network_event(10, &data).expect("decodes");

        assert_eq!(packet.pid, 1234);
        assert_eq!(packet.size, 1400);
        assert_eq!(packet.proto, Proto::Tcp);
        assert_eq!(packet.direction, Direction::Send);
        assert_eq!(packet.daddr, "93.184.216.34".parse::<IpAddr>().unwrap());
        assert_eq!(packet.saddr, "192.168.1.20".parse::<IpAddr>().unwrap());
        assert_eq!(packet.dport, 443);
        assert_eq!(packet.sport, 51000);
    }

    #[test]
    fn a_udp_v6_receive_decodes_and_unmaps_v4_addresses() {
        let mut data = Vec::new();
        data.extend(77u32.to_le_bytes());
        data.extend(1200u32.to_le_bytes());
        data.extend("2606:4700::1111".parse::<Ipv6Addr>().unwrap().octets());
        // A v4-mapped local address should come out as plain IPv4.
        data.extend("::ffff:10.0.0.5".parse::<Ipv6Addr>().unwrap().octets());
        data.extend(443u16.to_be_bytes());
        data.extend(60000u16.to_be_bytes());
        data.extend([0u8; 8]);

        let packet = network_event(59, &data).expect("decodes");
        assert_eq!(packet.proto, Proto::Udp);
        assert_eq!(packet.direction, Direction::Recv);
        assert_eq!(packet.daddr, "2606:4700::1111".parse::<IpAddr>().unwrap());
        assert_eq!(packet.saddr, "10.0.0.5".parse::<IpAddr>().unwrap());
        assert_eq!((packet.dport, packet.sport), (443, 60000));
    }

    #[test]
    fn truncated_or_foreign_events_are_rejected_not_misread() {
        let data = v4_payload(1, 2, [1, 1, 1, 1], [2, 2, 2, 2], 53, 5353);
        assert!(
            network_event(12, &data).is_none(),
            "connect events carry no bytes"
        );
        assert!(network_event(10, &data[..19]).is_none(), "missing sport");
        assert!(network_event(26, &data).is_none(), "too short for IPv6");
        assert!(network_event(10, &[]).is_none());
    }

    fn wide(text: &str) -> Vec<u8> {
        text.encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    fn dns_payload(name: &str, status: u32, results: &str) -> Vec<u8> {
        let mut data = wide(name);
        data.extend(1u32.to_le_bytes()); // QueryType A
        data.extend(0u64.to_le_bytes()); // QueryOptions
        data.extend(status.to_le_bytes());
        data.extend(wide(results));
        data
    }

    #[test]
    fn a_completed_query_maps_every_address_to_the_name_asked_for() {
        let data = dns_payload(
            "WWW.Example.com.",
            0,
            "type:  5 edge.example.net;::ffff:93.184.216.34;2606:2800:220:1::1;",
        );
        let (name, addresses) = dns_event(&data).expect("decodes");

        assert_eq!(name, "www.example.com", "lower-cased, root dot dropped");
        assert_eq!(
            addresses,
            vec![
                "93.184.216.34".parse::<IpAddr>().unwrap(),
                "2606:2800:220:1::1".parse::<IpAddr>().unwrap(),
            ]
        );
    }

    #[test]
    fn failed_or_addressless_queries_teach_nothing() {
        assert!(dns_event(&dns_payload("nx.example", 9003, "")).is_none());
        assert!(dns_event(&dns_payload("cname.example", 0, "type:  5 a.example;")).is_none());
        // A name with no terminator is a cut-short payload.
        let mut cut = wide("example.com");
        cut.truncate(cut.len() - 2);
        assert!(dns_event(&cut).is_none());
    }
}
