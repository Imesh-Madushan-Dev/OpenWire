//! Names for the addresses the monitor sees.
//!
//! The kernel reports traffic by IP; the DNS client reports which name each
//! IP was looked up as. Joining the two is what lets the Hosts column read
//! `cdn.example.com` rather than an address - for anything resolved through
//! Windows' own resolver, which is nearly everything. Apps with their own
//! DNS-over-HTTPS bypass it, and their traffic is listed by address instead.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

/// Addresses remembered per generation. Two generations make an LRU-ish
/// cache with no per-hit bookkeeping: lookups promote into the young one,
/// and when it fills the old one is simply dropped.
const GENERATION: usize = 4096;

#[derive(Debug, Default)]
pub struct DnsNames {
    young: HashMap<IpAddr, Arc<str>>,
    old: HashMap<IpAddr, Arc<str>>,
}

impl DnsNames {
    pub fn record(&mut self, name: &str, addresses: &[IpAddr]) {
        let name: Arc<str> = Arc::from(name);
        for address in addresses {
            self.insert(*address, Arc::clone(&name));
        }
    }

    pub fn lookup(&mut self, address: &IpAddr) -> Option<Arc<str>> {
        if let Some(name) = self.young.get(address) {
            return Some(Arc::clone(name));
        }
        // Still in use, so worth carrying into the next generation.
        let name = self.old.remove(address)?;
        self.insert(*address, Arc::clone(&name));
        Some(name)
    }

    fn insert(&mut self, address: IpAddr, name: Arc<str>) {
        if self.young.len() >= GENERATION && !self.young.contains_key(&address) {
            self.old = std::mem::take(&mut self.young);
        }
        self.young.insert(address, name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(n: u32) -> IpAddr {
        IpAddr::V4(n.into())
    }

    #[test]
    fn an_address_answers_with_the_latest_name_it_was_resolved_as() {
        let mut names = DnsNames::default();
        names.record("a.example", &[ip(1), ip(2)]);
        names.record("b.example", &[ip(2)]);

        assert_eq!(names.lookup(&ip(1)).as_deref(), Some("a.example"));
        assert_eq!(names.lookup(&ip(2)).as_deref(), Some("b.example"));
        assert_eq!(names.lookup(&ip(3)), None);
    }

    #[test]
    fn memory_stays_bounded_while_addresses_in_use_survive() {
        let mut names = DnsNames::default();
        names.record("kept.example", &[ip(0)]);

        for n in 1..(GENERATION as u32 * 3) {
            names.record("churn.example", &[ip(n)]);
            // Touched often enough to keep getting promoted.
            if n % 1000 == 0 {
                assert_eq!(names.lookup(&ip(0)).as_deref(), Some("kept.example"));
            }
        }

        assert!(names.young.len() + names.old.len() <= GENERATION * 2);
        assert_eq!(names.lookup(&ip(0)).as_deref(), Some("kept.example"));
    }
}
