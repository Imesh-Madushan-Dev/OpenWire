//! Who made a network card, from the first bits of its MAC address.
//!
//! The IEEE assigns address blocks to manufacturers in three sizes - 24, 28
//! and 36-bit prefixes - and publishes who holds each. The table is compiled in
//! (see `resources/oui/README.md`) and indexed on first use; the longest
//! matching prefix wins, since a small block carved out of a large one names
//! the company that actually built the device.

use std::collections::HashMap;
use std::sync::OnceLock;

static TABLE: &str = include_str!("../../../resources/oui/oui.tsv");

type Registry = [(u32, HashMap<u64, &'static str>); 3];

/// One map per prefix length in hex digits, longest first.
fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(build)
}

fn build() -> Registry {
    let mut maps = [(9, HashMap::new()), (7, HashMap::new()), (6, HashMap::with_capacity(40_000))];
    for line in TABLE.lines() {
        let Some((prefix, name)) = line.split_once('\t') else {
            continue;
        };
        let Ok(value) = u64::from_str_radix(prefix, 16) else {
            continue;
        };
        if let Some((_, map)) = maps.iter_mut().find(|(digits, _)| *digits as usize == prefix.len()) {
            map.insert(value, name);
        }
    }
    maps
}

/// The manufacturer a MAC address was assigned to, if the IEEE lists one.
pub fn vendor(mac: [u8; 6]) -> Option<&'static str> {
    if is_private(mac) {
        return None;
    }
    let value = mac.iter().fold(0u64, |acc, byte| (acc << 8) | u64::from(*byte));
    registry()
        .iter()
        .find_map(|(digits, map)| map.get(&(value >> (48 - digits * 4))).copied())
}

/// A locally administered address: made up by the device rather than burned
/// in by its maker. Phones do this per Wi-Fi network to avoid being tracked,
/// so there is no vendor to find - but it says a good deal about the device.
pub fn is_private(mac: [u8; 6]) -> bool {
    mac[0] & 0x02 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_longest_assigned_prefix_names_the_maker() {
        assert_eq!(vendor([0xF0, 0xB4, 0x79, 0x00, 0x00, 0x01]), Some("Apple"));
        assert_eq!(vendor([0x00, 0x11, 0x32, 0x09, 0x6E, 0x68]), Some("Synology"));
        // A 36-bit block inside a 24-bit one the IEEE holds for itself.
        assert_eq!(
            vendor([0x8C, 0x1F, 0x64, 0xAF, 0xA0, 0x00]),
            Some("Data Electronic Devices")
        );
    }

    #[test]
    fn randomised_addresses_have_no_maker() {
        let phone = [0xDA, 0xA1, 0x19, 0x00, 0x00, 0x01];
        assert!(is_private(phone));
        assert_eq!(vendor(phone), None);
    }
}
