//! What else is on this network.
//!
//! A scan finds every device on the local subnet with ARP - the request a
//! machine must answer to be reachable at all, so nothing short of being
//! switched off hides from it - then asks each one its name and looks up who
//! made its network card. Results are kept per network, keyed by the router's
//! hardware address, so a laptop that moves between home and office keeps two
//! separate lists, and a device seen for the first time is flagged until the
//! user has looked.
//!
//! Nothing here needs elevation, and nothing leaves the local network.

pub mod kind;
mod names;
mod sys;
mod vendor;

use std::collections::{BTreeMap, HashMap};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

pub use kind::DeviceKind;
pub use names::NameSource;
use names::Hostname;
use sys::Adapter;

use crate::core::store;
use crate::core::usage::unix_seconds;
use crate::error::{AppError, AppResult};

/// The widest subnet swept. A /22 is 1,022 addresses and a few seconds; an
/// office /16 would be 65,534, so wider networks are swept only around this
/// machine's own address.
const WIDEST_SWEEP: u8 = 22;
/// ARP requests in flight at once. Each holds a blocking thread until its
/// address answers or Windows gives up on it, about 3.5 s for an address
/// nobody holds - so a /24 fits in a single round.
const ARP_PARALLEL: usize = 256;
const NAME_PARALLEL: usize = 32;
/// How recent the last sweep must be for "online" to still mean something.
const ONLINE_FOR_SECS: i64 = 15 * 60;
/// Devices remembered per network; past this the longest-unseen are dropped.
const MAX_DEVICES: usize = 512;
const MAX_NAME_LEN: usize = 64;

// -- what is stored -----------------------------------------------------------

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Book {
    networks: BTreeMap<String, Network>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Network {
    subnet: String,
    gateway: String,
    first_scan: i64,
    /// When the last completed sweep started: a device seen since was there.
    last_scan: i64,
    /// Keyed by MAC address, which outlives a DHCP lease.
    devices: BTreeMap<String, Known>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Known {
    ip: String,
    hostname: Option<String>,
    name_source: Option<NameSource>,
    custom_name: Option<String>,
    custom_kind: Option<DeviceKind>,
    first_seen: i64,
    last_seen: i64,
    /// Cleared for devices that join after the first sweep, until looked at.
    acknowledged: bool,
}

// -- what the UI gets ---------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInfo {
    pub id: String,
    /// What Windows calls the adapter, e.g. "Wi-Fi".
    pub adapter: String,
    pub wireless: bool,
    /// CIDR, e.g. "192.168.1.0/24".
    pub subnet: String,
    /// This machine's address on it.
    pub address: String,
    pub gateway: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    Router,
    ThisDevice,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub mac: String,
    pub ip: String,
    /// What to call it: the user's name for it, else its own.
    pub name: Option<String>,
    /// The name it gave, and how it was learnt.
    pub hostname: Option<String>,
    pub name_source: Option<NameSource>,
    pub custom_name: Option<String>,
    pub vendor: Option<String>,
    /// A randomised address, so no vendor can be known.
    pub private_address: bool,
    pub kind: DeviceKind,
    pub kind_is_custom: bool,
    pub role: Option<Role>,
    pub online: bool,
    pub first_seen: i64,
    pub last_seen: i64,
    pub is_new: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicesSnapshot {
    /// False where scanning is not implemented, so the UI can say so.
    pub supported: bool,
    /// `None` when there is no LAN to scan: offline, or only a VPN or
    /// cellular link.
    pub network: Option<NetworkInfo>,
    pub devices: Vec<Device>,
    pub scanning: bool,
    pub last_scan: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub done: usize,
    pub total: usize,
}

pub struct ScanOutcome {
    pub snapshot: DevicesSnapshot,
    /// Devices that joined since the previous sweep of this network.
    pub joined: Vec<Device>,
}

// -- the service --------------------------------------------------------------

pub struct Lan {
    path: PathBuf,
    book: Mutex<Book>,
    /// The LAN as last looked up, with the key its devices are stored under.
    current: Mutex<Option<(NetworkInfo, Adapter)>>,
    scanning: AtomicBool,
}

impl Lan {
    pub fn load(path: PathBuf) -> Arc<Self> {
        let book = store::read_json(&path).ok().flatten().unwrap_or_default();
        Arc::new(Self {
            path,
            book: Mutex::new(book),
            current: Mutex::new(None),
            scanning: AtomicBool::new(false),
        })
    }

    pub fn is_scanning(&self) -> bool {
        self.scanning.load(Ordering::Acquire)
    }

    /// Looks up the LAN again - no traffic, a few milliseconds of blocking
    /// calls - so moving to another network shows that network's devices.
    pub fn refresh_network(&self) {
        let found = sys::primary_adapter().map(|adapter| {
            let gateway_mac = sys::neighbors(adapter.index)
                .into_iter()
                .find(|neighbor| neighbor.ip == adapter.gateway)
                .map(|neighbor| neighbor.mac);
            let key = {
                let mut book = lock(&self.book);
                key_for(&mut book, &adapter, gateway_mac)
            };
            (describe(&adapter, key), adapter)
        });
        *lock(&self.current) = found;
    }

    pub fn snapshot(&self) -> DevicesSnapshot {
        let current = lock(&self.current).clone();
        let book = lock(&self.book);
        let now = unix_seconds();

        let (network, mut devices, last_scan) = match current {
            Some((info, adapter)) => {
                let stored = book.networks.get(&info.id);
                let devices = stored
                    .map(|network| {
                        network
                            .devices
                            .iter()
                            .map(|(mac, known)| to_device(mac, known, network, &adapter, now))
                            .collect()
                    })
                    .unwrap_or_default();
                let last_scan = stored.map(|network| network.last_scan).filter(|t| *t > 0);
                (Some(info), devices, last_scan)
            }
            None => (None, Vec::new(), None),
        };

        // This machine and the router first, then whoever is here now.
        devices.sort_by_key(|device: &Device| {
            let role = match device.role {
                Some(Role::ThisDevice) => 0,
                Some(Role::Router) => 1,
                None => 2,
            };
            let ip = device.ip.parse::<Ipv4Addr>().map(u32::from).unwrap_or(u32::MAX);
            (role, !device.online, ip)
        });

        DevicesSnapshot {
            supported: cfg!(windows),
            network,
            devices,
            scanning: self.is_scanning(),
            last_scan,
        }
    }

    /// Sweeps the subnet, names what answered, and records it. One at a time:
    /// a second request while one runs is refused rather than queued.
    pub async fn scan(&self, progress: impl Fn(ScanProgress) + Send + Sync) -> AppResult<ScanOutcome> {
        if self.scanning.swap(true, Ordering::AcqRel) {
            return Err(AppError::Invalid("A scan is already running.".into()));
        }
        // Clears the flag however this returns, early errors included.
        struct Finished<'a>(&'a AtomicBool);
        impl Drop for Finished<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        let _finished = Finished(&self.scanning);

        let adapter = tokio::task::spawn_blocking(sys::primary_adapter)
            .await
            .map_err(|err| AppError::Internal(format!("adapter lookup: {err}")))?;
        let Some(adapter) = adapter else {
            *lock(&self.current) = None;
            return Ok(ScanOutcome {
                snapshot: self.snapshot(),
                joined: Vec::new(),
            });
        };

        let started = unix_seconds();
        let found = sweep(&adapter, &progress).await;
        let names = name_all(&adapter, &found).await;

        let gateway_mac = found.get(&adapter.gateway).copied();
        let (key, joined) = {
            let mut book = lock(&self.book);
            let key = key_for(&mut book, &adapter, gateway_mac);
            let joined = merge(&mut book, &key, &adapter, &found, &names, started, unix_seconds());
            if let Err(err) = store::write_json_compact(&self.path, &*book) {
                log::warn!("could not save the device list: {err}");
            }
            (key, joined)
        };
        *lock(&self.current) = Some((describe(&adapter, key), adapter));

        let snapshot = self.snapshot();
        let joined = snapshot
            .devices
            .iter()
            .filter(|device| joined.contains(&device.mac))
            .cloned()
            .collect();
        Ok(ScanOutcome { snapshot, joined })
    }

    pub fn rename(&self, mac: &str, name: Option<String>) -> AppResult<()> {
        let name = name
            .map(|name| name.trim().chars().take(MAX_NAME_LEN).collect::<String>())
            .filter(|name| !name.is_empty());
        self.edit(mac, |known| known.custom_name = name)
    }

    /// `None` goes back to the automatic guess.
    pub fn set_kind(&self, mac: &str, kind: Option<DeviceKind>) -> AppResult<()> {
        self.edit(mac, |known| known.custom_kind = kind)
    }

    /// Clears the "new" flag on one device, or on all of them.
    pub fn acknowledge(&self, mac: Option<&str>) -> AppResult<()> {
        self.with_network(|network| {
            match mac {
                Some(mac) => known_mut(network, mac)?.acknowledged = true,
                None => network
                    .devices
                    .values_mut()
                    .for_each(|known| known.acknowledged = true),
            }
            Ok(())
        })
    }

    /// Drops a device from the list. If it is still around, the next sweep
    /// finds it again - as a new device.
    pub fn forget(&self, mac: &str) -> AppResult<()> {
        self.with_network(|network| {
            network
                .devices
                .remove(&mac.to_ascii_lowercase())
                .map(|_| ())
                .ok_or_else(gone)
        })
    }

    fn edit(&self, mac: &str, change: impl FnOnce(&mut Known)) -> AppResult<()> {
        self.with_network(|network| {
            change(known_mut(network, mac)?);
            Ok(())
        })
    }

    fn with_network(&self, change: impl FnOnce(&mut Network) -> AppResult<()>) -> AppResult<()> {
        let key = lock(&self.current)
            .as_ref()
            .map(|(info, _)| info.id.clone())
            .ok_or_else(|| AppError::Invalid("You're not on a local network right now.".into()))?;
        let mut book = lock(&self.book);
        let network = book.networks.get_mut(&key).ok_or_else(gone)?;
        change(network)?;
        store::write_json_compact(&self.path, &*book)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    // A panic elsewhere must not take the device list down with it.
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn gone() -> AppError {
    AppError::Invalid("That device is no longer in the list.".into())
}

fn known_mut<'a>(network: &'a mut Network, mac: &str) -> AppResult<&'a mut Known> {
    network
        .devices
        .get_mut(&mac.to_ascii_lowercase())
        .ok_or_else(gone)
}

// -- scanning -----------------------------------------------------------------

/// Every address on the subnet that answers ARP, plus this machine.
async fn sweep(
    adapter: &Adapter,
    progress: &(impl Fn(ScanProgress) + Sync),
) -> HashMap<Ipv4Addr, [u8; 6]> {
    let targets = sweep_targets(adapter.ip, adapter.prefix);
    let total = targets.len();
    progress(ScanProgress { done: 0, total });

    let mut found = HashMap::new();
    // What the ARP cache confirmed moments ago counts without asking again.
    let index = adapter.index;
    if let Ok(cached) = tokio::task::spawn_blocking(move || sys::neighbors(index)).await {
        for neighbor in cached.into_iter().filter(|neighbor| neighbor.reachable) {
            found.insert(neighbor.ip, neighbor.mac);
        }
    }

    let permits = Arc::new(Semaphore::new(ARP_PARALLEL));
    let mut requests = JoinSet::new();
    for target in targets {
        let permits = Arc::clone(&permits);
        let source = adapter.ip;
        requests.spawn(async move {
            let _permit = permits.acquire_owned().await.ok()?;
            let mac = tokio::task::spawn_blocking(move || sys::arp(target, source))
                .await
                .ok()??;
            Some((target, mac))
        });
    }

    let mut done = 0;
    while let Some(result) = requests.join_next().await {
        done += 1;
        if let Ok(Some((ip, mac))) = result {
            found.insert(ip, mac);
        }
        // Often enough for a smooth bar, rarely enough not to flood the IPC.
        if done % 16 == 0 || done == total {
            progress(ScanProgress { done, total });
        }
    }

    if let Some(mac) = adapter.mac {
        found.insert(adapter.ip, mac);
    }
    found
}

/// Names for everything found. This machine names itself; the rest are
/// asked, with DNS put only to a server inside the subnet.
async fn name_all(adapter: &Adapter, found: &HashMap<Ipv4Addr, [u8; 6]>) -> HashMap<Ipv4Addr, Hostname> {
    let dns = adapter
        .dns
        .iter()
        .copied()
        .find(|server| same_subnet(*server, adapter.ip, adapter.prefix))
        .or(Some(adapter.gateway));

    let permits = Arc::new(Semaphore::new(NAME_PARALLEL));
    let mut lookups = JoinSet::new();
    for ip in found.keys().copied().filter(|ip| *ip != adapter.ip) {
        let permits = Arc::clone(&permits);
        lookups.spawn(async move {
            let _permit = permits.acquire_owned().await.ok()?;
            Some((ip, names::resolve(ip, dns).await?))
        });
    }

    let mut names = HashMap::new();
    while let Some(result) = lookups.join_next().await {
        if let Ok(Some((ip, hostname))) = result {
            names.insert(ip, hostname);
        }
    }
    if let Some(name) = std::env::var("COMPUTERNAME").ok().and_then(|name| names::tidy(&name)) {
        names.insert(
            adapter.ip,
            Hostname {
                name,
                source: NameSource::Netbios,
            },
        );
    }
    names
}

/// The addresses a sweep asks: the subnet, narrowed to a /22 around this
/// machine when wider, without the network, broadcast or own address.
fn sweep_targets(ip: Ipv4Addr, prefix: u8) -> Vec<Ipv4Addr> {
    let prefix = prefix.clamp(WIDEST_SWEEP, 30);
    let mask = u32::MAX << (32 - u32::from(prefix));
    let network = u32::from(ip) & mask;
    let broadcast = network | !mask;
    (network + 1..broadcast)
        .map(Ipv4Addr::from)
        .filter(|host| *host != ip)
        .collect()
}

fn same_subnet(a: Ipv4Addr, b: Ipv4Addr, prefix: u8) -> bool {
    let mask = u32::MAX << (32 - u32::from(prefix));
    u32::from(a) & mask == u32::from(b) & mask
}

fn subnet_label(ip: Ipv4Addr, prefix: u8) -> String {
    let mask = u32::MAX << (32 - u32::from(prefix));
    format!("{}/{prefix}", Ipv4Addr::from(u32::from(ip) & mask))
}

// -- bookkeeping ----------------------------------------------------------------

/// The key a network's devices live under: its router's hardware address,
/// which survives the router handing out a new subnet. Before that is known,
/// a stored network with the same subnet and gateway is reused, and adopted
/// under the router's address once it is.
fn key_for(book: &mut Book, adapter: &Adapter, gateway_mac: Option<[u8; 6]>) -> String {
    let subnet = subnet_label(adapter.ip, adapter.prefix);
    let gateway = adapter.gateway.to_string();
    let provisional = format!("net-{subnet}");

    let Some(mac) = gateway_mac else {
        return book
            .networks
            .iter()
            .find(|(_, network)| network.subnet == subnet && network.gateway == gateway)
            .map(|(key, _)| key.clone())
            .unwrap_or(provisional);
    };

    let key = format!("gw-{}", mac_text(mac));
    if !book.networks.contains_key(&key) {
        if let Some(network) = book.networks.remove(&provisional) {
            book.networks.insert(key.clone(), network);
        }
    }
    key
}

/// Folds one sweep into the book, returning the MACs of devices that are new
/// to this network. On the first sweep nothing is "new": it was all there
/// before OpenWire looked.
fn merge(
    book: &mut Book,
    key: &str,
    adapter: &Adapter,
    found: &HashMap<Ipv4Addr, [u8; 6]>,
    names: &HashMap<Ipv4Addr, Hostname>,
    started: i64,
    finished: i64,
) -> Vec<String> {
    let network = book.networks.entry(key.to_string()).or_insert_with(|| Network {
        first_scan: started,
        ..Network::default()
    });
    network.subnet = subnet_label(adapter.ip, adapter.prefix);
    network.gateway = adapter.gateway.to_string();
    let baseline = network.last_scan == 0;

    let mut joined = Vec::new();
    for (ip, mac) in found {
        let mac = mac_text(*mac);
        let fresh = !network.devices.contains_key(&mac);
        let known = network.devices.entry(mac.clone()).or_insert_with(|| Known {
            first_seen: started,
            acknowledged: baseline,
            ..Known::default()
        });
        known.ip = ip.to_string();
        known.last_seen = finished;
        // A name, once learnt, survives a sweep where the device stays quiet.
        if let Some(hostname) = names.get(ip) {
            known.hostname = Some(hostname.name.clone());
            known.name_source = Some(hostname.source);
        }
        if fresh && !baseline {
            joined.push(mac);
        }
    }
    network.last_scan = started;

    if network.devices.len() > MAX_DEVICES {
        let mut by_age: Vec<(i64, String)> = network
            .devices
            .iter()
            .map(|(mac, known)| (known.last_seen, mac.clone()))
            .collect();
        by_age.sort();
        for (_, mac) in by_age.into_iter().take(network.devices.len() - MAX_DEVICES) {
            network.devices.remove(&mac);
        }
    }
    joined
}

fn describe(adapter: &Adapter, id: String) -> NetworkInfo {
    NetworkInfo {
        id,
        adapter: adapter.name.clone(),
        wireless: adapter.wireless,
        subnet: subnet_label(adapter.ip, adapter.prefix),
        address: adapter.ip.to_string(),
        gateway: adapter.gateway.to_string(),
    }
}

fn to_device(mac_key: &str, known: &Known, network: &Network, adapter: &Adapter, now: i64) -> Device {
    let mac = parse_mac(mac_key).unwrap_or_default();
    let ip = known.ip.parse::<Ipv4Addr>().ok();
    let role = if ip == Some(adapter.ip) || adapter.mac == Some(mac) {
        Some(Role::ThisDevice)
    } else if ip == Some(adapter.gateway) {
        Some(Role::Router)
    } else {
        None
    };

    let private_address = vendor::is_private(mac);
    let vendor = vendor::vendor(mac);
    let kind = known.custom_kind.unwrap_or_else(|| match role {
        Some(Role::Router) => DeviceKind::Router,
        Some(Role::ThisDevice) => DeviceKind::Computer,
        None => kind::guess(known.hostname.as_deref(), vendor, private_address),
    });
    let online = network.last_scan > 0
        && known.last_seen >= network.last_scan
        && now - network.last_scan <= ONLINE_FOR_SECS;

    Device {
        mac: mac_key.to_string(),
        ip: known.ip.clone(),
        name: known.custom_name.clone().or_else(|| known.hostname.clone()),
        hostname: known.hostname.clone(),
        name_source: known.name_source,
        custom_name: known.custom_name.clone(),
        vendor: vendor.map(str::to_string),
        private_address,
        kind,
        kind_is_custom: known.custom_kind.is_some(),
        role,
        online,
        first_seen: known.first_seen,
        last_seen: known.last_seen,
        is_new: !known.acknowledged,
    }
}

fn mac_text(mac: [u8; 6]) -> String {
    mac.iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn parse_mac(text: &str) -> Option<[u8; 6]> {
    let mut mac = [0u8; 6];
    let mut parts = text.split(':');
    for byte in &mut mac {
        *byte = u8::from_str_radix(parts.next()?, 16).ok()?;
    }
    parts.next().is_none().then_some(mac)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> Adapter {
        Adapter {
            index: 7,
            name: "Wi-Fi".into(),
            wireless: true,
            mac: Some([0x10, 0, 0, 0, 0, 0x05]),
            ip: Ipv4Addr::new(192, 168, 1, 5),
            prefix: 24,
            gateway: Ipv4Addr::new(192, 168, 1, 1),
            dns: vec![Ipv4Addr::new(192, 168, 1, 1)],
        }
    }

    const ROUTER: [u8; 6] = [0x50, 0xC7, 0xBF, 0, 0, 1];
    const PHONE: [u8; 6] = [0xDA, 0xA1, 0x19, 0, 0, 2];

    #[test]
    fn a_sweep_covers_the_subnet_but_never_a_whole_office() {
        let home = sweep_targets(Ipv4Addr::new(192, 168, 1, 5), 24);
        assert_eq!(home.len(), 253, "254 hosts, less this machine");
        assert!(!home.contains(&Ipv4Addr::new(192, 168, 1, 0)));
        assert!(!home.contains(&Ipv4Addr::new(192, 168, 1, 255)));

        let office = sweep_targets(Ipv4Addr::new(10, 20, 30, 40), 16);
        assert_eq!(office.len(), 1021, "narrowed to the /22 around us");
        assert!(office.contains(&Ipv4Addr::new(10, 20, 31, 1)));
    }

    #[test]
    fn nothing_is_new_on_the_first_sweep_and_newcomers_are_after_it() {
        let mut book = Book::default();
        let adapter = adapter();
        let key = key_for(&mut book, &adapter, Some(ROUTER));

        let first = HashMap::from([(adapter.gateway, ROUTER)]);
        assert!(merge(&mut book, &key, &adapter, &first, &HashMap::new(), 100, 105).is_empty());

        let mut second = first.clone();
        second.insert(Ipv4Addr::new(192, 168, 1, 42), PHONE);
        let joined = merge(&mut book, &key, &adapter, &second, &HashMap::new(), 400, 405);
        assert_eq!(joined, vec![mac_text(PHONE)]);

        let network = &book.networks[&key];
        let device = to_device(&mac_text(PHONE), &network.devices[&mac_text(PHONE)], network, &adapter, 410);
        assert!(device.is_new && device.online && device.private_address);
        assert_eq!(device.kind, DeviceKind::Phone, "a private address reads as a phone");

        let router = to_device(&mac_text(ROUTER), &network.devices[&mac_text(ROUTER)], network, &adapter, 410);
        assert_eq!(router.role, Some(Role::Router));
        assert!(!router.is_new);
        assert_eq!(router.vendor.as_deref(), Some("TP-Link Technologies"));
    }

    #[test]
    fn devices_missing_from_the_latest_sweep_are_offline() {
        let mut book = Book::default();
        let adapter = adapter();
        let key = key_for(&mut book, &adapter, Some(ROUTER));
        let phone_ip = Ipv4Addr::new(192, 168, 1, 42);

        merge(&mut book, &key, &adapter, &HashMap::from([(phone_ip, PHONE)]), &HashMap::new(), 100, 105);
        merge(&mut book, &key, &adapter, &HashMap::new(), &HashMap::new(), 400, 405);

        let network = &book.networks[&key];
        let phone = to_device(&mac_text(PHONE), &network.devices[&mac_text(PHONE)], network, &adapter, 410);
        assert!(!phone.online);
        assert_eq!(phone.last_seen, 105);

        let stale = to_device(&mac_text(PHONE), &network.devices[&mac_text(PHONE)], network, &adapter, 100 + ONLINE_FOR_SECS + 10_000);
        assert!(!stale.online, "an old sweep says nothing about now");
    }

    #[test]
    fn a_network_found_before_its_router_was_keeps_its_devices() {
        let mut book = Book::default();
        let adapter = adapter();
        let provisional = key_for(&mut book, &adapter, None);
        merge(&mut book, &provisional, &adapter, &HashMap::from([(adapter.gateway, ROUTER)]), &HashMap::new(), 1, 2);

        assert_eq!(key_for(&mut book, &adapter, None), provisional, "found again by subnet");
        let key = key_for(&mut book, &adapter, Some(ROUTER));
        assert_eq!(book.networks.len(), 1);
        assert_eq!(book.networks[&key].devices.len(), 1);
    }

    /// Sweeps the real network and prints what it found. Run by hand:
    /// `cargo test --lib live_sweep -- --ignored --nocapture`
    #[test]
    #[ignore = "sends ARP requests on the local network"]
    fn live_sweep() {
        let path = std::env::temp_dir().join(format!("openwire-devices-{}.json", std::process::id()));
        let lan = Lan::load(path.clone());
        let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();

        let began = std::time::Instant::now();
        let outcome = runtime.block_on(lan.scan(|_| {})).unwrap();
        let _ = std::fs::remove_file(path);

        println!("network: {:?}", outcome.snapshot.network);
        println!("{} devices in {:?}", outcome.snapshot.devices.len(), began.elapsed());
        for device in &outcome.snapshot.devices {
            println!(
                "{:<15} {}  {:<22} {:<28} {:?}{}",
                device.ip,
                device.mac,
                device.name.as_deref().unwrap_or("-"),
                device.vendor.as_deref().unwrap_or(if device.private_address { "(private address)" } else { "-" }),
                device.kind,
                device.role.map(|role| format!("  [{role:?}]")).unwrap_or_default(),
            );
        }
    }

    #[test]
    fn macs_round_trip_through_text() {
        assert_eq!(mac_text(ROUTER), "50:c7:bf:00:00:01");
        assert_eq!(parse_mac("50:c7:bf:00:00:01"), Some(ROUTER));
        assert_eq!(parse_mac("50:c7:bf:00:00"), None);
    }
}
