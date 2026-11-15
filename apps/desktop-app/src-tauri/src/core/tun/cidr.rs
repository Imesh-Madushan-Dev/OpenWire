//! Which destinations are routed into the adapter.
//!
//! Everything, minus what must stay out: the server (or the tunnel would try
//! to carry itself), loopback, multicast, link-local space, and private
//! networks when the LAN bypass is on. Rather than route `0.0.0.0/0` and pin
//! the exceptions to the old gateway, the exceptions are cut out of the
//! table - the same arithmetic WireGuard's AllowedIPs calculator does. What
//! is left out simply follows the machine's own default route, so roaming to
//! another network needs no update here.
//!
//! The routes are never wider than /1: a /0 on the adapter would tie with
//! the physical default route and leave the choice to interface metrics.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cidr {
    pub address: IpAddr,
    pub prefix: u8,
}

impl Cidr {
    pub fn new(address: IpAddr, prefix: u8) -> Self {
        let bits = bits_of(&address);
        let prefix = prefix.min(bits);
        Self {
            address: from_value(&address, value(&address) & mask(bits, prefix)),
            prefix,
        }
    }

    pub fn host(address: IpAddr) -> Self {
        Self::new(address, bits_of(&address))
    }

    pub fn contains(&self, other: &Cidr) -> bool {
        self.address.is_ipv4() == other.address.is_ipv4()
            && self.prefix <= other.prefix
            && value(&other.address) & mask(bits_of(&self.address), self.prefix)
                == value(&self.address)
    }

    fn overlaps(&self, other: &Cidr) -> bool {
        self.contains(other) || other.contains(self)
    }

    /// The two halves one bit longer.
    fn halves(&self) -> [Cidr; 2] {
        let bits = bits_of(&self.address);
        let prefix = self.prefix + 1;
        let high = value(&self.address) | (1u128 << (bits - prefix));
        [
            Cidr::new(self.address, prefix),
            Cidr::new(from_value(&self.address, high), prefix),
        ]
    }
}

impl std::fmt::Display for Cidr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.address, self.prefix)
    }
}

fn bits_of(address: &IpAddr) -> u8 {
    if address.is_ipv4() {
        32
    } else {
        128
    }
}

fn value(address: &IpAddr) -> u128 {
    match address {
        IpAddr::V4(v4) => u32::from(*v4) as u128,
        IpAddr::V6(v6) => u128::from(*v6),
    }
}

fn from_value(like: &IpAddr, value: u128) -> IpAddr {
    match like {
        IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::from(value as u32)),
        IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::from(value)),
    }
}

fn mask(bits: u8, prefix: u8) -> u128 {
    if prefix == 0 {
        return 0;
    }
    let all = if bits == 32 {
        u32::MAX as u128
    } else {
        u128::MAX
    };
    // At most 127 host bits once the prefix is at least 1, so this never
    // shifts a u128 by its full width.
    all & !((1u128 << (bits - prefix)) - 1)
}

/// `range` with `hole` cut out, as the fewest CIDRs that cover the rest.
fn subtract(range: Cidr, hole: &Cidr, out: &mut Vec<Cidr>) {
    if !range.overlaps(hole) {
        out.push(range);
    } else if !hole.contains(&range) {
        for half in range.halves() {
            subtract(half, hole, out);
        }
    }
}

/// `universe` minus every hole, never wider than /1.
fn complement(universe: Cidr, holes: &[Cidr]) -> Vec<Cidr> {
    let mut ranges = vec![universe];
    for hole in holes {
        let mut next = Vec::with_capacity(ranges.len() + 32);
        for range in ranges {
            subtract(range, hole, &mut next);
        }
        ranges = next;
    }
    ranges
        .into_iter()
        .flat_map(|range| {
            if range.prefix == 0 {
                range.halves().to_vec()
            } else {
                vec![range]
            }
        })
        .collect()
}

fn net(text: &str, prefix: u8) -> Cidr {
    Cidr::new(text.parse().expect("a literal address"), prefix)
}

/// Never routed into the adapter, whatever the settings.
fn always_excluded_v4() -> Vec<Cidr> {
    vec![
        net("0.0.0.0", 8),      // "this network"
        net("127.0.0.0", 8),    // loopback
        net("169.254.0.0", 16), // link-local, including DHCP's fallback
        net("224.0.0.0", 4),    // multicast
        net("240.0.0.0", 4),    // reserved, and the limited broadcast
    ]
}

/// Private address space, kept local by the LAN bypass.
fn private_v4() -> Vec<Cidr> {
    vec![
        net("10.0.0.0", 8),
        net("172.16.0.0", 12),
        net("192.168.0.0", 16),
    ]
}

/// IPv4 routes for the adapter.
pub fn routes_v4(exclude: &[IpAddr], lan_bypass: bool) -> Vec<Cidr> {
    let mut holes = always_excluded_v4();
    if lan_bypass {
        holes.extend(private_v4());
    }
    holes.extend(
        exclude
            .iter()
            .filter(|ip| ip.is_ipv4())
            .map(|ip| Cidr::host(*ip)),
    );
    complement(net("0.0.0.0", 0), &holes)
}

/// IPv6 routes for the adapter: global unicast (all public IPv6 lives in
/// 2000::/3), plus unique local space unless the LAN bypass keeps it local.
/// Link-local and multicast are never touched.
pub fn routes_v6(exclude: &[IpAddr], lan_bypass: bool) -> Vec<Cidr> {
    let holes: Vec<Cidr> = exclude
        .iter()
        .filter(|ip| ip.is_ipv6())
        .map(|ip| Cidr::host(*ip))
        .collect();
    let mut routes = complement(net("2000::", 3), &holes);
    if !lan_bypass {
        // The adapter's own /126 sits in here; its on-link route is more
        // specific, so it still wins.
        routes.extend(complement(net("fc00::", 7), &holes));
    }
    routes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    fn covered(routes: &[Cidr], address: &str) -> bool {
        let host = Cidr::host(ip(address));
        routes.iter().any(|route| route.contains(&host))
    }

    #[test]
    fn cidrs_normalise_and_contain() {
        let block = net("192.168.1.77", 24);
        assert_eq!(block.to_string(), "192.168.1.0/24");
        assert!(block.contains(&Cidr::host(ip("192.168.1.200"))));
        assert!(!block.contains(&Cidr::host(ip("192.168.2.1"))));
        assert!(net("0.0.0.0", 0).contains(&block));
        assert!(!block.contains(&net("0.0.0.0", 0)));
        assert!(
            !block.contains(&Cidr::host(ip("::1"))),
            "families never mix"
        );
        assert_eq!(net("2001:db8::1", 32).to_string(), "2001:db8::/32");
    }

    #[test]
    fn with_nothing_to_leave_out_the_default_route_is_split_in_two() {
        let routes = complement(net("0.0.0.0", 0), &[]);
        assert_eq!(routes, vec![net("0.0.0.0", 1), net("128.0.0.0", 1)]);
    }

    #[test]
    fn the_server_is_routed_around_and_everything_else_is_not() {
        let routes = routes_v4(&[ip("203.0.113.10")], true);

        assert!(
            !covered(&routes, "203.0.113.10"),
            "the server stays on the real network"
        );
        assert!(covered(&routes, "203.0.113.11"));
        assert!(covered(&routes, "203.0.113.9"));
        for public in ["1.1.1.1", "8.8.8.8", "142.250.80.46", "100.64.0.1"] {
            assert!(covered(&routes, public), "{public} should be tunnelled");
        }
        for local in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.5.4",
            "192.168.1.1",
            "169.254.10.10",
            "224.0.0.251",
            "255.255.255.255",
        ] {
            assert!(!covered(&routes, local), "{local} should stay local");
        }
        assert!(
            routes.iter().all(|route| route.prefix >= 1),
            "never a /0 that ties with the real default route"
        );
    }

    #[test]
    fn without_the_lan_bypass_private_space_is_tunnelled() {
        let routes = routes_v4(&[], false);
        assert!(covered(&routes, "10.1.2.3"));
        assert!(covered(&routes, "192.168.1.1"));
        assert!(!covered(&routes, "127.0.0.1"), "loopback never");
        assert!(!covered(&routes, "224.0.0.251"), "multicast never");
    }

    #[test]
    fn the_routes_never_overlap_and_stay_few() {
        let excluded: Vec<IpAddr> = (1..=8).map(|n| ip(&format!("198.51.100.{n}"))).collect();
        let routes = routes_v4(&excluded, true);
        for (i, a) in routes.iter().enumerate() {
            for b in &routes[i + 1..] {
                assert!(!a.overlaps(b), "{a} overlaps {b}");
            }
        }
        // Each /32 hole costs at most 32 routes; this is far below that.
        assert!(routes.len() < 120, "{} routes", routes.len());
    }

    #[test]
    fn ipv6_covers_public_space_and_routes_around_the_server() {
        let routes = routes_v6(&[ip("2001:db8::10"), ip("203.0.113.10")], true);
        assert!(covered(&routes, "2606:4700::1111"));
        assert!(!covered(&routes, "2001:db8::10"));
        assert!(covered(&routes, "2001:db8::11"));
        assert!(!covered(&routes, "fe80::1"), "link-local never");
        assert!(!covered(&routes, "ff02::fb"), "multicast never");
        assert!(
            !covered(&routes, "fd00::1"),
            "unique local stays local with the bypass"
        );

        let routes = routes_v6(&[], false);
        assert!(covered(&routes, "fd00::1"));
    }
}
