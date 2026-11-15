//! Leak protection with the Windows Filtering Platform.
//!
//! Routes decide where traffic should go; these filters decide what may go
//! anywhere else. Two sets, both in a sublayer of their own:
//!
//! - **DNS**, always while the adapter is up. Windows asks every network's
//!   DNS servers in parallel ("smart multi-homed name resolution"), so the
//!   adapter's server being first is not enough: queries to port 53 are
//!   blocked everywhere except through the adapter and loopback.
//! - **Kill switch**, when the user turns it on: nothing leaves except
//!   through the adapter, from the app itself (it reaches the server
//!   directly), over loopback, for DHCP and IPv6 neighbour discovery, and -
//!   with the LAN bypass on - to the local network.
//!
//! The filters are added in a *dynamic* session: Windows deletes them when
//! the session's handle closes, including when the process dies without
//! closing it. A crash can never leave the machine walled off.
//!
//! The permits here only exempt traffic from these blocks; they never
//! override a block from Windows Firewall or anything else. The blocks, on
//! the other hand, win over other software's permits - which is the point.

use std::io;
use std::path::Path;

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{HANDLE, NO_ERROR};
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::{
    FwpmEngineClose0, FwpmEngineOpen0, FwpmFilterAdd0, FwpmFreeMemory0, FwpmGetAppIdFromFileName0,
    FwpmSubLayerAdd0, FwpmTransactionAbort0, FwpmTransactionBegin0, FwpmTransactionCommit0,
    FWPM_ACTION0, FWPM_CONDITION_ALE_APP_ID, FWPM_CONDITION_FLAGS,
    FWPM_CONDITION_IP_LOCAL_INTERFACE, FWPM_CONDITION_IP_LOCAL_PORT, FWPM_CONDITION_IP_PROTOCOL,
    FWPM_CONDITION_IP_REMOTE_ADDRESS, FWPM_CONDITION_IP_REMOTE_PORT, FWPM_DISPLAY_DATA0,
    FWPM_FILTER0, FWPM_FILTER_CONDITION0, FWPM_LAYER_ALE_AUTH_CONNECT_V4,
    FWPM_LAYER_ALE_AUTH_CONNECT_V6, FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V4,
    FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V6, FWPM_SESSION0, FWPM_SESSION_FLAG_DYNAMIC, FWPM_SUBLAYER0,
    FWP_ACTION_BLOCK, FWP_ACTION_PERMIT, FWP_ACTION_TYPE, FWP_BYTE_BLOB, FWP_BYTE_BLOB_TYPE,
    FWP_CONDITION_FLAG_IS_LOOPBACK, FWP_CONDITION_VALUE0, FWP_MATCH_EQUAL, FWP_MATCH_FLAGS_ALL_SET,
    FWP_UINT16, FWP_UINT32, FWP_UINT64, FWP_UINT8, FWP_V4_ADDR_AND_MASK, FWP_V4_ADDR_MASK,
    FWP_V6_ADDR_AND_MASK, FWP_V6_ADDR_MASK, FWP_VALUE0,
};

use super::cidr::Cidr;
use crate::core::winsvc::wide;

/// NTLM: the only authentication a local BFE session uses.
const RPC_C_AUTHN_WINNT: u32 = 10;

const UDP: u8 = 17;
const ICMPV6: u8 = 58;

/// Filter weights inside the sublayer; the highest matching one decides.
mod weight {
    pub const PERMIT_ADAPTER: u8 = 15;
    pub const PERMIT_LOOPBACK: u8 = 14;
    pub const BLOCK_DNS: u8 = 13;
    pub const PERMIT_APP: u8 = 12;
    pub const PERMIT_ESSENTIAL: u8 = 12;
    pub const PERMIT_LAN: u8 = 11;
    pub const BLOCK_ALL: u8 = 0;
}

pub struct Rules<'a> {
    /// The adapter's LUID.
    pub interface: u64,
    /// The app, which reaches the server outside the adapter.
    pub app: &'a Path,
    pub kill_switch: bool,
    pub lan_bypass: bool,
}

/// The open filtering session. Dropping it removes every filter.
pub struct Firewall {
    engine: HANDLE,
}

// SAFETY: a BFE engine handle may be used and closed from any thread.
unsafe impl Send for Firewall {}

impl Firewall {
    pub fn enable(rules: &Rules) -> io::Result<Self> {
        let mut name = wide("OpenWire");
        let mut description = wide("OpenWire system-wide routing leak protection");
        let session = FWPM_SESSION0 {
            displayData: FWPM_DISPLAY_DATA0 {
                name: name.as_mut_ptr(),
                description: description.as_mut_ptr(),
            },
            flags: FWPM_SESSION_FLAG_DYNAMIC,
            ..FWPM_SESSION0::default()
        };

        let mut engine: HANDLE = std::ptr::null_mut();
        // SAFETY: a local engine (null server), the session outliving the
        // call, and a valid out-pointer.
        check(
            unsafe {
                FwpmEngineOpen0(
                    std::ptr::null(),
                    RPC_C_AUTHN_WINNT,
                    std::ptr::null(),
                    &session,
                    &mut engine,
                )
            },
            "open the filtering engine",
        )?;
        let firewall = Self { engine };

        // All or nothing: a half-installed set of blocks is worse than none.
        check(unsafe { FwpmTransactionBegin0(engine, 0) }, "begin")?;
        match firewall.install(rules) {
            Ok(()) => check(unsafe { FwpmTransactionCommit0(engine) }, "commit")?,
            Err(err) => {
                unsafe { FwpmTransactionAbort0(engine) };
                return Err(err);
            }
        }
        Ok(firewall)
    }

    fn install(&self, rules: &Rules) -> io::Result<()> {
        let sublayer = GUID::from_u128(uuid::Uuid::new_v4().as_u128());
        let mut name = wide("OpenWire");
        let mut description = wide("OpenWire system-wide routing");
        let layer = FWPM_SUBLAYER0 {
            subLayerKey: sublayer,
            displayData: FWPM_DISPLAY_DATA0 {
                name: name.as_mut_ptr(),
                description: description.as_mut_ptr(),
            },
            // Above every other sublayer, so these are evaluated first.
            weight: u16::MAX,
            ..FWPM_SUBLAYER0::default()
        };
        // SAFETY: the sublayer and its strings outlive the call.
        check(
            unsafe { FwpmSubLayerAdd0(self.engine, &layer, std::ptr::null_mut()) },
            "add sublayer",
        )?;

        let filters = Filters {
            engine: self.engine,
            sublayer,
        };
        let outbound = [
            FWPM_LAYER_ALE_AUTH_CONNECT_V4,
            FWPM_LAYER_ALE_AUTH_CONNECT_V6,
        ];
        let every = [
            FWPM_LAYER_ALE_AUTH_CONNECT_V4,
            FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V4,
            FWPM_LAYER_ALE_AUTH_CONNECT_V6,
            FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V6,
        ];

        for layer in every {
            filters.add(
                layer,
                "permit the adapter",
                weight::PERMIT_ADAPTER,
                FWP_ACTION_PERMIT,
                &[Condition::Interface(rules.interface)],
            )?;
            filters.add(
                layer,
                "permit loopback",
                weight::PERMIT_LOOPBACK,
                FWP_ACTION_PERMIT,
                &[Condition::Loopback],
            )?;
        }
        for layer in outbound {
            filters.add(
                layer,
                "block DNS outside the adapter",
                weight::BLOCK_DNS,
                FWP_ACTION_BLOCK,
                &[Condition::RemotePort(53)],
            )?;
        }

        if !rules.kill_switch {
            return Ok(());
        }

        let app = AppId::of(rules.app)?;
        for layer in every {
            filters.add(
                layer,
                "permit OpenWire",
                weight::PERMIT_APP,
                FWP_ACTION_PERMIT,
                &[Condition::App(app.blob())],
            )?;
            filters.add(
                layer,
                "block everything else",
                weight::BLOCK_ALL,
                FWP_ACTION_BLOCK,
                &[],
            )?;
        }

        // DHCP, so the real network keeps its lease while the switch holds.
        for layer in [
            FWPM_LAYER_ALE_AUTH_CONNECT_V4,
            FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V4,
        ] {
            filters.add(
                layer,
                "permit DHCP",
                weight::PERMIT_ESSENTIAL,
                FWP_ACTION_PERMIT,
                &[
                    Condition::Protocol(UDP),
                    Condition::LocalPort(68),
                    Condition::RemotePort(67),
                ],
            )?;
        }
        for layer in [
            FWPM_LAYER_ALE_AUTH_CONNECT_V6,
            FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V6,
        ] {
            filters.add(
                layer,
                "permit DHCPv6",
                weight::PERMIT_ESSENTIAL,
                FWP_ACTION_PERMIT,
                &[
                    Condition::Protocol(UDP),
                    Condition::LocalPort(546),
                    Condition::RemotePort(547),
                ],
            )?;
            // Neighbour and router discovery: without them IPv6 on the real
            // network stops working entirely.
            filters.add(
                layer,
                "permit neighbour discovery",
                weight::PERMIT_ESSENTIAL,
                FWP_ACTION_PERMIT,
                &[
                    Condition::Protocol(ICMPV6),
                    Condition::Remote(net("fe80::", 10)),
                    Condition::Remote(net("ff00::", 8)),
                ],
            )?;
        }

        if rules.lan_bypass {
            let v4: Vec<Condition> = [
                ("10.0.0.0", 8),
                ("172.16.0.0", 12),
                ("192.168.0.0", 16),
                ("169.254.0.0", 16),
                ("224.0.0.0", 4),
                ("255.255.255.255", 32),
            ]
            .iter()
            .map(|(address, prefix)| Condition::Remote(net(address, *prefix)))
            .collect();
            let v6: Vec<Condition> = [("fe80::", 10), ("fc00::", 7), ("ff00::", 8)]
                .iter()
                .map(|(address, prefix)| Condition::Remote(net(address, *prefix)))
                .collect();
            for layer in [
                FWPM_LAYER_ALE_AUTH_CONNECT_V4,
                FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V4,
            ] {
                filters.add(
                    layer,
                    "permit the local network",
                    weight::PERMIT_LAN,
                    FWP_ACTION_PERMIT,
                    &v4,
                )?;
            }
            for layer in [
                FWPM_LAYER_ALE_AUTH_CONNECT_V6,
                FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V6,
            ] {
                filters.add(
                    layer,
                    "permit the local network",
                    weight::PERMIT_LAN,
                    FWP_ACTION_PERMIT,
                    &v6,
                )?;
            }
        }
        Ok(())
    }
}

impl Drop for Firewall {
    fn drop(&mut self) {
        // SAFETY: opened in `enable`, closed once. Closing a dynamic session
        // deletes everything added in it.
        unsafe { FwpmEngineClose0(self.engine) };
    }
}

fn net(address: &str, prefix: u8) -> Cidr {
    Cidr::new(address.parse().expect("a literal address"), prefix)
}

fn check(code: u32, what: &str) -> io::Result<()> {
    if code == NO_ERROR {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "firewall: {what} failed (0x{code:08X})"
        )))
    }
}

/// What a filter matches on. Conditions on the same field are OR'd, on
/// different fields AND'd - which is how WFP reads them.
enum Condition {
    Interface(u64),
    Loopback,
    Protocol(u8),
    LocalPort(u16),
    RemotePort(u16),
    Remote(Cidr),
    App(*mut FWP_BYTE_BLOB),
}

struct Filters {
    engine: HANDLE,
    sublayer: GUID,
}

impl Filters {
    fn add(
        &self,
        layer: GUID,
        name: &str,
        weight: u8,
        action: FWP_ACTION_TYPE,
        conditions: &[Condition],
    ) -> io::Result<()> {
        // Values the conditions point at, sized up front so no push moves
        // them while a pointer to them is held.
        let mut u64s: Vec<u64> = Vec::with_capacity(conditions.len());
        let mut v4s: Vec<FWP_V4_ADDR_AND_MASK> = Vec::with_capacity(conditions.len());
        let mut v6s: Vec<FWP_V6_ADDR_AND_MASK> = Vec::with_capacity(conditions.len());
        let mut raw: Vec<FWPM_FILTER_CONDITION0> = Vec::with_capacity(conditions.len());

        for condition in conditions {
            let mut value = FWP_CONDITION_VALUE0::default();
            let (field, matching) = match condition {
                Condition::Interface(luid) => {
                    u64s.push(*luid);
                    value.r#type = FWP_UINT64;
                    value.Anonymous.uint64 = u64s.last_mut().unwrap();
                    (FWPM_CONDITION_IP_LOCAL_INTERFACE, FWP_MATCH_EQUAL)
                }
                Condition::Loopback => {
                    value.r#type = FWP_UINT32;
                    value.Anonymous.uint32 = FWP_CONDITION_FLAG_IS_LOOPBACK;
                    (FWPM_CONDITION_FLAGS, FWP_MATCH_FLAGS_ALL_SET)
                }
                Condition::Protocol(protocol) => {
                    value.r#type = FWP_UINT8;
                    value.Anonymous.uint8 = *protocol;
                    (FWPM_CONDITION_IP_PROTOCOL, FWP_MATCH_EQUAL)
                }
                Condition::LocalPort(port) => {
                    value.r#type = FWP_UINT16;
                    value.Anonymous.uint16 = *port;
                    (FWPM_CONDITION_IP_LOCAL_PORT, FWP_MATCH_EQUAL)
                }
                Condition::RemotePort(port) => {
                    value.r#type = FWP_UINT16;
                    value.Anonymous.uint16 = *port;
                    (FWPM_CONDITION_IP_REMOTE_PORT, FWP_MATCH_EQUAL)
                }
                Condition::Remote(cidr) => {
                    match cidr.address {
                        std::net::IpAddr::V4(v4) => {
                            // Host byte order, unlike almost everything else.
                            let mask = if cidr.prefix == 0 {
                                0
                            } else {
                                u32::MAX << (32 - cidr.prefix)
                            };
                            v4s.push(FWP_V4_ADDR_AND_MASK {
                                addr: u32::from(v4),
                                mask,
                            });
                            value.r#type = FWP_V4_ADDR_MASK;
                            value.Anonymous.v4AddrMask = v4s.last_mut().unwrap();
                        }
                        std::net::IpAddr::V6(v6) => {
                            v6s.push(FWP_V6_ADDR_AND_MASK {
                                addr: v6.octets(),
                                prefixLength: cidr.prefix,
                            });
                            value.r#type = FWP_V6_ADDR_MASK;
                            value.Anonymous.v6AddrMask = v6s.last_mut().unwrap();
                        }
                    }
                    (FWPM_CONDITION_IP_REMOTE_ADDRESS, FWP_MATCH_EQUAL)
                }
                Condition::App(blob) => {
                    value.r#type = FWP_BYTE_BLOB_TYPE;
                    value.Anonymous.byteBlob = *blob;
                    (FWPM_CONDITION_ALE_APP_ID, FWP_MATCH_EQUAL)
                }
            };
            raw.push(FWPM_FILTER_CONDITION0 {
                fieldKey: field,
                matchType: matching,
                conditionValue: value,
            });
        }

        let mut display = wide(format!("OpenWire: {name}"));
        let filter = FWPM_FILTER0 {
            displayData: FWPM_DISPLAY_DATA0 {
                name: display.as_mut_ptr(),
                description: std::ptr::null_mut(),
            },
            layerKey: layer,
            subLayerKey: self.sublayer,
            weight: FWP_VALUE0 {
                r#type: FWP_UINT8,
                Anonymous:
                    windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWP_VALUE0_0 {
                        uint8: weight,
                    },
            },
            numFilterConditions: raw.len() as u32,
            filterCondition: if raw.is_empty() {
                std::ptr::null_mut()
            } else {
                raw.as_mut_ptr()
            },
            action: FWPM_ACTION0 {
                r#type: action,
                ..FWPM_ACTION0::default()
            },
            ..FWPM_FILTER0::default()
        };

        let mut id = 0u64;
        // SAFETY: the filter, its conditions and everything they point at
        // live until this call returns.
        check(
            unsafe { FwpmFilterAdd0(self.engine, &filter, std::ptr::null_mut(), &mut id) },
            name,
        )
    }
}

/// The identity WFP gives an executable, which is its device path.
struct AppId(*mut FWP_BYTE_BLOB);

impl AppId {
    fn of(path: &Path) -> io::Result<Self> {
        let file = wide(path);
        let mut blob: *mut FWP_BYTE_BLOB = std::ptr::null_mut();
        // SAFETY: a NUL-terminated path and a valid out-pointer.
        check(
            unsafe { FwpmGetAppIdFromFileName0(file.as_ptr(), &mut blob) },
            "identify the app",
        )?;
        Ok(Self(blob))
    }

    fn blob(&self) -> *mut FWP_BYTE_BLOB {
        self.0
    }
}

impl Drop for AppId {
    fn drop(&mut self) {
        // SAFETY: allocated by FwpmGetAppIdFromFileName0.
        unsafe {
            FwpmFreeMemory0(&mut self.0 as *mut *mut FWP_BYTE_BLOB as *mut *mut core::ffi::c_void)
        };
    }
}
