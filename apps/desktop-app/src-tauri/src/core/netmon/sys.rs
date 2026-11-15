//! Small Win32 helpers shared by the service, its installer and the client.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub use crate::core::winsvc::wide;

/// Every unicast address assigned to this machine, for telling the local end
/// of a flow from the remote one.
pub fn local_addresses() -> HashSet<IpAddr> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        FreeMibTable, GetUnicastIpAddressTable, MIB_UNICASTIPADDRESS_ROW,
        MIB_UNICASTIPADDRESS_TABLE,
    };
    use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6, AF_UNSPEC};

    let mut table: *mut MIB_UNICASTIPADDRESS_TABLE = std::ptr::null_mut();
    // SAFETY: on success the OS hands back a table it owns, released below.
    if unsafe { GetUnicastIpAddressTable(AF_UNSPEC, &mut table) } != 0 || table.is_null() {
        return HashSet::new();
    }

    let mut out = HashSet::new();
    // SAFETY: `NumEntries` rows follow the header contiguously, and each row's
    // address union is tagged by `si_family`.
    unsafe {
        let rows = std::slice::from_raw_parts(
            std::ptr::addr_of!((*table).Table).cast::<MIB_UNICASTIPADDRESS_ROW>(),
            (*table).NumEntries as usize,
        );
        for row in rows {
            match row.Address.si_family {
                AF_INET => {
                    // Network byte order as stored, so the in-memory bytes are
                    // already the address's octets.
                    let raw = row.Address.Ipv4.sin_addr.S_un.S_addr;
                    out.insert(IpAddr::V4(Ipv4Addr::from(raw.to_ne_bytes())));
                }
                AF_INET6 => {
                    let octets = row.Address.Ipv6.sin6_addr.u.Byte;
                    out.insert(IpAddr::V6(Ipv6Addr::from(octets)).to_canonical());
                }
                _ => {}
            }
        }
        FreeMibTable(table.cast());
    }
    out
}

pub fn init_service_log() {
    crate::core::winsvc::log::init(&super::scm::SPEC);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_machine_has_addresses_and_a_program_files() {
        let local = local_addresses();
        assert!(
            local.iter().any(|ip| ip.is_loopback()),
            "loopback is always assigned: {local:?}"
        );

        let exe = super::super::scm::SPEC.exe().expect("known folder resolves");
        assert!(exe.starts_with(crate::core::winsvc::program_files().unwrap()));
        assert!(exe.ends_with("OpenWire Network Monitor/openwire-netmon.exe"));
    }

    #[test]
    fn querying_a_missing_service_is_quiet() {
        // Not installed on a build machine; and when it is, the answer still
        // has to be self-consistent.
        let state = super::super::scm::query();
        assert!(state.installed || !state.running);
    }
}
