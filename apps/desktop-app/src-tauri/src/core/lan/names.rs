//! Finding out what a device calls itself.
//!
//! No single protocol names everything on a home network, so three are asked
//! at once and the most trustworthy answer wins:
//!
//! - **mDNS** (port 5353), asked of the device directly - Apple devices,
//!   printers, Chromecasts and most smart speakers answer with the name their
//!   owner gave them ("Imeshs-iPhone.local");
//! - **NetBIOS** (port 137) - Windows PCs and Samba NAS boxes;
//! - **reverse DNS** against the router - whatever hostname the device sent
//!   when it asked for its address over DHCP.
//!
//! Every query stays on the local network: DNS is only ever asked of a server
//! inside the scanned subnet, so no lookup of a private address leaks to a
//! public resolver.

use std::net::{Ipv4Addr, SocketAddr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::time::{timeout_at, Instant};

/// How long each protocol gets. Devices that answer do so in milliseconds;
/// the wait only matters for the ones that never will.
const WAIT: Duration = Duration::from_millis(900);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NameSource {
    Mdns,
    Netbios,
    Dns,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hostname {
    pub name: String,
    pub source: NameSource,
}

/// The device's own name, by whichever protocol answers best.
pub async fn resolve(ip: Ipv4Addr, dns_server: Option<Ipv4Addr>) -> Option<Hostname> {
    let (mdns, netbios, dns) = tokio::join!(
        ptr(SocketAddr::from((ip, 5353)), ip, false),
        netbios(ip),
        async {
            match dns_server {
                Some(server) => ptr(SocketAddr::from((server, 53)), ip, true).await,
                None => None,
            }
        },
    );

    [(mdns, NameSource::Mdns), (netbios, NameSource::Netbios), (dns, NameSource::Dns)]
        .into_iter()
        .find_map(|(name, source)| {
            let name = tidy(&name?)?;
            Some(Hostname { name, source })
        })
}

/// What is worth showing of a raw name: the first label, without `.local` or
/// the router's search domain, and nothing at all for the placeholder names
/// routers invent for devices that never said ("unknown8851fb156ff7").
pub fn tidy(raw: &str) -> Option<String> {
    let first = raw.trim().trim_end_matches('.').split('.').next()?.trim();
    let lower = first.to_ascii_lowercase();
    let placeholder = lower.is_empty()
        || lower == "localhost"
        || (lower.starts_with("unknown") && lower[7..].chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
        || lower.chars().all(|c| c.is_ascii_digit() || c == '-');
    (!placeholder).then(|| first.chars().take(63).collect())
}

fn query_id(ip: Ipv4Addr) -> u16 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos as u16) ^ (u32::from(ip) as u16)
}

/// One request, one reply from the address it was sent to.
async fn exchange(target: SocketAddr, packet: &[u8]) -> Option<Vec<u8>> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await.ok()?;
    socket.send_to(packet, target).await.ok()?;

    let deadline = Instant::now() + WAIT;
    let mut buf = vec![0u8; 1500];
    loop {
        // An ICMP "port unreachable" surfaces here as an error, which ends
        // the wait early for devices that do not speak the protocol.
        let (len, from) = timeout_at(deadline, socket.recv_from(&mut buf)).await.ok()?.ok()?;
        if from.ip() == target.ip() {
            buf.truncate(len);
            return Some(buf);
        }
    }
}

// -- DNS / mDNS ---------------------------------------------------------------

/// Asks `server` for the PTR record of `ip`. Recursion is only requested of a
/// real DNS server; an mDNS responder answers for itself.
async fn ptr(server: SocketAddr, ip: Ipv4Addr, recursion: bool) -> Option<String> {
    let id = query_id(ip);
    let reply = exchange(server, &ptr_query(id, ip, recursion)).await?;
    parse_ptr(&reply, id)
}

fn ptr_query(id: u16, ip: Ipv4Addr, recursion: bool) -> Vec<u8> {
    let [a, b, c, d] = ip.octets();
    let name = format!("{d}.{c}.{b}.{a}.in-addr.arpa");

    let mut packet = Vec::with_capacity(48);
    packet.extend_from_slice(&id.to_be_bytes());
    packet.extend_from_slice(&(if recursion { 0x0100u16 } else { 0 }).to_be_bytes());
    // One question, no answer, authority or additional records.
    packet.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0]);
    for label in name.split('.') {
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    // Root, then QTYPE PTR, QCLASS IN.
    packet.extend_from_slice(&[0, 0, 12, 0, 1]);
    packet
}

/// The first PTR answer in a DNS reply.
fn parse_ptr(msg: &[u8], id: u16) -> Option<String> {
    let word = |at: usize| Some(u16::from_be_bytes([*msg.get(at)?, *msg.get(at + 1)?]));

    // Some mDNS responders zero the ID rather than echo it.
    let reply_id = word(0)?;
    if reply_id != id && reply_id != 0 {
        return None;
    }
    let flags = word(2)?;
    if flags & 0x8000 == 0 || flags & 0x000F != 0 {
        return None; // not a response, or an error
    }

    let mut at = 12;
    for _ in 0..word(4)? {
        at = read_name(msg, at)?.1 + 4;
    }
    for _ in 0..word(6)? {
        let (_, after) = read_name(msg, at)?;
        let kind = word(after)?;
        let length = usize::from(word(after + 8)?);
        let data = after + 10;
        if kind == 12 {
            return read_name(msg, data).map(|(name, _)| name).filter(|name| !name.is_empty());
        }
        at = data + length;
    }
    None
}

/// A possibly compressed name at `at`, and the offset just past it.
fn read_name(msg: &[u8], mut at: usize) -> Option<(String, usize)> {
    let mut labels: Vec<String> = Vec::new();
    let mut end = None;
    // A pointer loop in a malformed reply must not spin forever.
    for _ in 0..64 {
        let len = usize::from(*msg.get(at)?);
        if len & 0xC0 == 0xC0 {
            end.get_or_insert(at + 2);
            at = ((len & 0x3F) << 8) | usize::from(*msg.get(at + 1)?);
        } else if len == 0 {
            return Some((labels.join("."), *end.get_or_insert(at + 1)));
        } else {
            labels.push(String::from_utf8_lossy(msg.get(at + 1..at + 1 + len)?).into_owned());
            at += 1 + len;
        }
    }
    None
}

// -- NetBIOS ------------------------------------------------------------------

/// The workstation name from a NetBIOS node status request.
async fn netbios(ip: Ipv4Addr) -> Option<String> {
    let id = query_id(ip);
    let reply = exchange(SocketAddr::from((ip, 137)), &netbios_query(id)).await?;
    parse_netbios(&reply)
}

fn netbios_query(id: u16) -> Vec<u8> {
    let mut packet = Vec::with_capacity(50);
    packet.extend_from_slice(&id.to_be_bytes());
    packet.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
    // The wildcard name "*", NUL-padded to 16 bytes, in NetBIOS's
    // half-byte-per-letter encoding.
    packet.push(32);
    let mut name = [0u8; 16];
    name[0] = b'*';
    for byte in name {
        packet.push(b'A' + (byte >> 4));
        packet.push(b'A' + (byte & 0x0F));
    }
    // Root, then NBSTAT, IN.
    packet.extend_from_slice(&[0, 0, 0x21, 0, 1]);
    packet
}

fn parse_netbios(msg: &[u8]) -> Option<String> {
    // Header, then the echoed name (34 bytes, or a 2-byte pointer).
    let mut at = 12;
    at += if *msg.get(at)? & 0xC0 == 0xC0 { 2 } else { 34 };
    // TYPE, CLASS, TTL, RDLENGTH.
    at += 10;
    let count = usize::from(*msg.get(at)?);
    at += 1;

    (0..count).find_map(|index| {
        let entry = msg.get(at + index * 18..at + index * 18 + 18)?;
        let suffix = entry[15];
        let group = entry[16] & 0x80 != 0;
        // Suffix 0x00 on a unique name is the machine's own name; group
        // names are workgroups and domains.
        (suffix == 0 && !group)
            .then(|| String::from_utf8_lossy(&entry[..15]).trim().to_string())
            .filter(|name| !name.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_names_are_not_names() {
        assert_eq!(tidy("unknown8851fb156ff7.attlocal.net"), None);
        assert_eq!(tidy("192-168-1-20"), None);
        assert_eq!(tidy("dslDevice.attlocal.net").as_deref(), Some("dslDevice"));
        assert_eq!(tidy("Imeshs-iPhone.local.").as_deref(), Some("Imeshs-iPhone"));
    }

    #[test]
    fn a_ptr_answer_is_read_through_name_compression() {
        let id = 0x1234;
        let mut reply = ptr_query(id, Ipv4Addr::new(192, 168, 1, 7), false);
        reply[2] = 0x84; // response, authoritative
        reply[7] = 1; // one answer
        // Owner name: a pointer back to the question's name at offset 12.
        reply.extend_from_slice(&[0xC0, 12, 0, 12, 0, 1, 0, 0, 0, 120]);
        let rdata = [&[7][..], b"printer", &[5], b"local", &[0]].concat();
        reply.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        reply.extend_from_slice(&rdata);

        assert_eq!(parse_ptr(&reply, id).as_deref(), Some("printer.local"));
        assert_eq!(parse_ptr(&reply, id + 1), None, "a reply to someone else's question");
    }

    #[test]
    fn the_workstation_name_is_picked_from_a_node_status_reply() {
        let mut reply = vec![0u8; 12];
        reply.push(0x20);
        reply.extend_from_slice(&[b'A'; 32]);
        reply.push(0);
        reply.extend_from_slice(&[0, 0x21, 0, 1, 0, 0, 0, 0, 0, 0]);
        reply.push(2);
        let entry = |name: &str, suffix: u8, flags: u8| {
            let mut bytes = format!("{name:<15}").into_bytes();
            bytes.push(suffix);
            bytes.extend_from_slice(&[flags, 0]);
            bytes
        };
        reply.extend(entry("WORKGROUP", 0, 0x80));
        reply.extend(entry("OFFICE-PC", 0, 0x04));

        assert_eq!(parse_netbios(&reply).as_deref(), Some("OFFICE-PC"));
    }
}
