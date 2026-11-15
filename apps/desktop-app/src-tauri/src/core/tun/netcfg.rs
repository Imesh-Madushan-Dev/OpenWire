//! Addressing the adapter, pointing routes at it and giving it a DNS server,
//! through the IP Helper API rather than shelling out to `netsh`.
//!
//! Everything set here lives on the adapter itself, so it disappears with
//! the adapter - including when the service dies without cleaning up. Routes
//! are still removed explicitly on a clean stop, before anything else, so
//! traffic returns to the real network at once rather than briefly
//! blackholing while the rest is taken down.

use std::io;
use std::net::IpAddr;

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, ERROR_OBJECT_ALREADY_EXISTS, NO_ERROR};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    ConvertInterfaceLuidToGuid, ConvertInterfaceLuidToIndex, CreateIpForwardEntry2,
    CreateUnicastIpAddressEntry, DeleteIpForwardEntry2, GetIpInterfaceEntry,
    InitializeIpForwardEntry, InitializeIpInterfaceEntry, InitializeUnicastIpAddressEntry,
    SetIpInterfaceEntry, DNS_INTERFACE_SETTINGS, DNS_INTERFACE_SETTINGS_VERSION1,
    DNS_SETTING_NAMESERVER, DNS_SETTING_REGISTRATION_ENABLED, MIB_IPFORWARD_ROW2,
    MIB_IPINTERFACE_ROW, MIB_UNICASTIPADDRESS_ROW,
};
use windows_sys::Win32::NetworkManagement::Ndis::NET_LUID_LH;
use windows_sys::Win32::Networking::WinSock::{
    IpDadStatePreferred, RouterDiscoveryDisabled, AF_INET, AF_INET6, MIB_IPPROTO_NETMGMT,
    SOCKADDR_INET,
};

use super::cidr::Cidr;
use crate::core::winsvc::wide;

fn check(code: u32, what: &str) -> io::Result<()> {
    if code == NO_ERROR {
        Ok(())
    } else {
        Err(io::Error::new(
            io::Error::from_raw_os_error(code as i32).kind(),
            format!("{what}: {}", io::Error::from_raw_os_error(code as i32)),
        ))
    }
}

fn sockaddr(address: IpAddr) -> SOCKADDR_INET {
    // SAFETY: all-zero is a valid SOCKADDR_INET; the family tag is set below
    // together with the matching member.
    let mut raw: SOCKADDR_INET = unsafe { std::mem::zeroed() };
    match address {
        IpAddr::V4(v4) => {
            raw.Ipv4.sin_family = AF_INET;
            // Network byte order: the octets as they sit in memory.
            raw.Ipv4.sin_addr.S_un.S_addr = u32::from_ne_bytes(v4.octets());
        }
        IpAddr::V6(v6) => {
            raw.Ipv6.sin6_family = AF_INET6;
            raw.Ipv6.sin6_addr.u.Byte = v6.octets();
        }
    }
    raw
}

/// The LUID as one number.
pub fn luid_value(luid: &NET_LUID_LH) -> u64 {
    // SAFETY: the union's members are a u64 and bitfields over the same 64
    // bits, so every bit pattern is a valid `Value`.
    unsafe { luid.Value }
}

pub fn interface_index(luid: &NET_LUID_LH) -> io::Result<u32> {
    let mut index = 0u32;
    // SAFETY: valid pointers for the call.
    check(
        unsafe { ConvertInterfaceLuidToIndex(luid, &mut index) },
        "interface index",
    )?;
    Ok(index)
}

fn interface_guid(luid: &NET_LUID_LH) -> io::Result<GUID> {
    let mut guid = GUID::from_u128(0);
    // SAFETY: valid pointers for the call.
    check(
        unsafe { ConvertInterfaceLuidToGuid(luid, &mut guid) },
        "interface guid",
    )?;
    Ok(guid)
}

/// Lowest metric, the given MTU, and for IPv6 none of the autoconfiguration
/// a point-to-point adapter has no use for (and which would delay its
/// address by a duplicate-address check).
pub fn prepare(luid: &NET_LUID_LH, ipv6: bool, mtu: u16) -> io::Result<()> {
    let mut row = MIB_IPINTERFACE_ROW::default();
    // SAFETY: initialises the row in place.
    unsafe { InitializeIpInterfaceEntry(&mut row) };
    row.Family = if ipv6 { AF_INET6 } else { AF_INET };
    row.InterfaceLuid = *luid;
    // SAFETY: family and LUID identify the row to read.
    check(unsafe { GetIpInterfaceEntry(&mut row) }, "read interface")?;

    // The lowest metric puts this adapter's DNS server first in line.
    row.UseAutomaticMetric = false;
    row.Metric = 0;
    row.NlMtu = mtu as u32;
    if ipv6 {
        row.DadTransmits = 0;
        row.ManagedAddressConfigurationSupported = false;
        row.OtherStatefulConfigurationSupported = false;
        row.RouterDiscoveryBehavior = RouterDiscoveryDisabled;
    } else {
        // SetIpInterfaceEntry rejects IPv4 rows with anything else here.
        row.SitePrefixLength = 0;
    }
    // SAFETY: a row read back from the system, edited in place.
    check(
        unsafe { SetIpInterfaceEntry(&mut row) },
        "configure interface",
    )
}

pub fn add_address(luid: &NET_LUID_LH, address: IpAddr, prefix: u8) -> io::Result<()> {
    let mut row = MIB_UNICASTIPADDRESS_ROW::default();
    // SAFETY: initialises the row in place, lifetimes infinite.
    unsafe { InitializeUnicastIpAddressEntry(&mut row) };
    row.InterfaceLuid = *luid;
    row.Address = sockaddr(address);
    row.OnLinkPrefixLength = prefix;
    row.DadState = IpDadStatePreferred;
    // SAFETY: a fully initialised row.
    match unsafe { CreateUnicastIpAddressEntry(&row) } {
        ERROR_OBJECT_ALREADY_EXISTS => Ok(()),
        code => check(code, "add address"),
    }
}

/// Routes on the adapter. Dropping this removes them.
pub struct Routes {
    rows: Vec<MIB_IPFORWARD_ROW2>,
}

// SAFETY: plain data; the rows hold no pointers.
unsafe impl Send for Routes {}

impl Routes {
    pub fn add(luid: &NET_LUID_LH, cidrs: &[Cidr]) -> io::Result<Self> {
        let mut routes = Self {
            rows: Vec::with_capacity(cidrs.len()),
        };
        for cidr in cidrs {
            let mut row = MIB_IPFORWARD_ROW2::default();
            // SAFETY: initialises the row in place.
            unsafe { InitializeIpForwardEntry(&mut row) };
            row.InterfaceLuid = *luid;
            row.DestinationPrefix.Prefix = sockaddr(cidr.address);
            row.DestinationPrefix.PrefixLength = cidr.prefix;
            // On-link: an unspecified next hop of the same family.
            let unspecified = if cidr.address.is_ipv4() {
                IpAddr::from([0u8; 4])
            } else {
                IpAddr::from([0u8; 16])
            };
            row.NextHop = sockaddr(unspecified);
            row.Metric = 0;
            row.Protocol = MIB_IPPROTO_NETMGMT;
            // SAFETY: a fully initialised row.
            match unsafe { CreateIpForwardEntry2(&row) } {
                NO_ERROR | ERROR_OBJECT_ALREADY_EXISTS => routes.rows.push(row),
                // Dropping `routes` takes back the ones already added.
                code => return Err(check(code, &format!("add route {cidr}")).unwrap_err()),
            }
        }
        Ok(routes)
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }
}

impl Drop for Routes {
    fn drop(&mut self) {
        for row in &self.rows {
            // SAFETY: the same row the route was created from.
            match unsafe { DeleteIpForwardEntry2(row) } {
                // Already gone with the adapter.
                NO_ERROR | ERROR_NOT_FOUND => {}
                code => log::warn!("could not remove a route ({code})"),
            }
        }
    }
}

/// Gives the adapter its DNS servers. `SetInterfaceDnsSettings` exists from
/// Windows 10 2004; it is looked up at run time so the binary still starts on
/// older builds, which get the registry value it writes instead.
pub fn set_dns(luid: &NET_LUID_LH, servers: &[IpAddr]) -> io::Result<()> {
    let guid = interface_guid(luid)?;
    let v4: Vec<String> = servers
        .iter()
        .filter(|ip| ip.is_ipv4())
        .map(IpAddr::to_string)
        .collect();
    let list = v4.join(",");

    if let Some(set) = set_interface_dns_settings() {
        let mut name_servers = wide(&list);
        let settings = DNS_INTERFACE_SETTINGS {
            Version: DNS_INTERFACE_SETTINGS_VERSION1,
            // Registration off: the adapter's private address has no business
            // in a company's DNS.
            Flags: (DNS_SETTING_NAMESERVER | DNS_SETTING_REGISTRATION_ENABLED) as u64,
            NameServer: name_servers.as_mut_ptr(),
            RegistrationEnabled: 0,
            ..DNS_INTERFACE_SETTINGS::default()
        };
        // SAFETY: the settings and the string they point at outlive the call.
        return check(unsafe { set(guid, &settings) }, "set DNS");
    }

    let key = format!(
        r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{}",
        guid_string(&guid)
    );
    winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(&key, winreg::enums::KEY_WRITE)
        .and_then(|interface| interface.set_value("NameServer", &list.replace(',', " ")))
}

type SetInterfaceDnsSettingsFn =
    unsafe extern "system" fn(interface: GUID, settings: *const DNS_INTERFACE_SETTINGS) -> u32;

fn set_interface_dns_settings() -> Option<SetInterfaceDnsSettingsFn> {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};

    let module = wide("iphlpapi.dll");
    // SAFETY: iphlpapi is already loaded by the IP Helper imports above, so
    // this takes no reference and needs no FreeLibrary.
    let handle = unsafe { GetModuleHandleW(module.as_ptr()) };
    if handle.is_null() {
        return None;
    }
    // SAFETY: a loaded module and a NUL-terminated name. The export, where it
    // exists, has the signature netioapi.h gives it.
    unsafe { GetProcAddress(handle, c"SetInterfaceDnsSettings".as_ptr().cast()) }
        .map(|function| unsafe { std::mem::transmute::<_, SetInterfaceDnsSettingsFn>(function) })
}

fn guid_string(guid: &GUID) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        guid.data1,
        guid.data2,
        guid.data3,
        guid.data4[0],
        guid.data4[1],
        guid.data4[2],
        guid.data4[3],
        guid.data4[4],
        guid.data4[5],
        guid.data4[6],
        guid.data4[7]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guids_format_the_way_the_registry_names_interfaces() {
        let guid = GUID::from_u128(0x4f1e2a9c_7d3b_4e5f_9a61_0c2b8d7e6f31);
        assert_eq!(guid_string(&guid), "{4F1E2A9C-7D3B-4E5F-9A61-0C2B8D7E6F31}");
    }

    #[test]
    fn socket_addresses_carry_their_family_and_bytes() {
        let v4 = sockaddr("172.19.0.1".parse().unwrap());
        // SAFETY: the family was just set to IPv4.
        unsafe {
            assert_eq!(v4.si_family, AF_INET);
            assert_eq!(v4.Ipv4.sin_addr.S_un.S_addr.to_ne_bytes(), [172, 19, 0, 1]);
        }
        let v6 = sockaddr("fdfe:dcba:9876::1".parse().unwrap());
        unsafe {
            assert_eq!(v6.si_family, AF_INET6);
            assert_eq!(v6.Ipv6.sin6_addr.u.Byte[0], 0xfd);
        }
    }

    #[test]
    fn the_dynamic_dns_entry_point_resolves_on_supported_windows() {
        // Every Windows 11 and current Windows 10 has it; this guards the
        // run-time lookup itself, not the fallback.
        assert!(set_interface_dns_settings().is_some());
    }
}
