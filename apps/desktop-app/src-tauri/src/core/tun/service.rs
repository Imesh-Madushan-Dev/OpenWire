//! The tunnel service: a control pipe, and at most one routing session.
//!
//! A session is bound to the pipe connection that started it. When the app
//! closes the pipe - deliberately, or by exiting or crashing - the session is
//! taken down at once. Routing every app at a proxy that no longer exists
//! would leave the machine with no network at all.

use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, WriteHalf};
use tokio::net::windows::named_pipe::NamedPipeServer;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use super::cidr::{self, Cidr};
use super::engine::{self, PacketWriter};
use super::firewall::{Firewall, Rules};
use super::netcfg::{self, Routes};
use super::wintun::{self, Adapter, Received, Wintun};
use super::{
    scm, verify, Event, Request, StartRequest, ADAPTER_NAME, DEFAULT_DNS, DNS_V4, INTERFACE_V4,
    INTERFACE_V6, MAX_DNS, MAX_EXCLUDED, MTU, PIPE_NAME, PREFIX_V4, PREFIX_V6, PROTOCOL,
    SERVICE_NAME,
};
use crate::core::winsvc::pipe::{self as winpipe, PipeSecurity};
use crate::core::winsvc::{self, host};

/// Fixed, so Windows sees the same network every time rather than a new one
/// per connection.
const ADAPTER_GUID: u128 = 0x5e3c1f0a_8b2d_4c7e_9f16_a4d3b2c1e0f9;
/// Packets buffered between Windows and the stack, each way. WireGuard's own
/// choice for the same driver.
const RING: u32 = 0x80_0000;
/// No request comes near this; a client that sends more is broken.
const MAX_LINE: u64 = 64 * 1024;
/// How often the service checks the app it belongs to still exists.
const OWNER_CHECK: Duration = Duration::from_secs(3600);

/// Entry point for `--tunnel-service`.
pub fn run() -> i32 {
    winsvc::log::init(&scm::SPEC);
    host::run(host::Service {
        name: SERVICE_NAME,
        main,
        on_stop: || {},
    })
}

fn main(stop: watch::Receiver<bool>) {
    // Two workers: one stack, its flows, and the pipe are not much work, but
    // a second keeps the pipe answering while the stack is busy.
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("tunnel")
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            log::error!("could not build the runtime: {err}");
            return;
        }
    };
    runtime.block_on(serve(stop, winsvc::owner_from_args()));
    // Flows still winding down are not worth waiting for.
    runtime.shutdown_timeout(Duration::from_secs(2));
}

struct Shared {
    /// The app executable this service was installed from: the only one it
    /// takes requests from.
    owner: Option<PathBuf>,
    /// Loaded on first use and kept: loading it again per session gains
    /// nothing.
    wintun: Mutex<Option<Arc<Wintun>>>,
    /// The client whose session is up, if any. One adapter, one session.
    active: Mutex<Option<u64>>,
}

impl Shared {
    fn wintun(&self) -> Result<Arc<Wintun>, String> {
        let mut slot = self.wintun.lock().map_err(|_| internal())?;
        if let Some(wintun) = slot.as_ref() {
            return Ok(Arc::clone(wintun));
        }
        let path = scm::SPEC
            .dir()
            .map(|dir| dir.join(scm::WINTUN_DLL))
            .ok_or_else(internal)?;
        let wintun = Wintun::load(&path).map_err(|err| {
            log::error!("loading {}: {err}", path.display());
            "The network driver couldn't be loaded. Repair the tunnel service in OpenWire's settings."
                .to_string()
        })?;
        *slot = Some(Arc::clone(&wintun));
        Ok(wintun)
    }

    fn claim(&self, client: u64) -> bool {
        let Ok(mut active) = self.active.lock() else {
            return false;
        };
        if active.is_some() {
            return false;
        }
        *active = Some(client);
        true
    }

    fn release(&self, client: u64) {
        if let Ok(mut active) = self.active.lock() {
            if *active == Some(client) {
                *active = None;
            }
        }
    }
}

fn internal() -> String {
    "System-wide routing hit an internal error. Try again.".to_string()
}

async fn serve(stop: watch::Receiver<bool>, owner: Option<PathBuf>) {
    if owner.is_none() {
        log::warn!("installed without an owner; no client will be accepted");
    }
    let shared = Arc::new(Shared {
        owner: owner.clone(),
        wintun: Mutex::new(None),
        active: Mutex::new(None),
    });
    tokio::join!(accept_loop(shared, stop.clone()), watch_owner(owner, stop),);
}

/// Removes the service once the app it belongs to has been uninstalled.
async fn watch_owner(owner: Option<PathBuf>, mut stop: watch::Receiver<bool>) {
    let Some(owner) = owner else { return };
    let mut missing_before = false;
    loop {
        tokio::select! {
            _ = tokio::time::sleep(OWNER_CHECK) => {}
            _ = stop.changed() => return,
        }
        // Twice in a row, an hour apart: an app update swaps the file for a
        // moment, and that must not read as an uninstall.
        let missing = !owner.exists();
        if missing && missing_before {
            log::info!("the app is gone; removing the tunnel service");
            scm::remove_self();
            host::request_stop();
            return;
        }
        missing_before = missing;
    }
}

async fn accept_loop(shared: Arc<Shared>, mut stop: watch::Receiver<bool>) {
    let security = match PipeSecurity::new(winpipe::SDDL_READ_WRITE) {
        Ok(security) => security,
        Err(err) => {
            log::error!("pipe security descriptor: {err}");
            return;
        }
    };

    let next_client = AtomicU64::new(1);
    let mut first = true;
    loop {
        let server = match security.create(PIPE_NAME, first, winpipe::Direction::Duplex) {
            Ok(server) => server,
            Err(err) => {
                // Most likely another process squatting on the name.
                log::error!("could not create the pipe: {err}");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(10)) => continue,
                    _ = stop.changed() => return,
                }
            }
        };
        first = false;

        tokio::select! {
            connected = server.connect() => {
                if connected.is_ok() {
                    let id = next_client.fetch_add(1, Ordering::Relaxed);
                    tokio::spawn(serve_client(server, id, Arc::clone(&shared), stop.clone()));
                }
            }
            _ = stop.changed() => return,
        }
    }
}

async fn serve_client(
    pipe: NamedPipeServer,
    id: u64,
    shared: Arc<Shared>,
    mut stop: watch::Receiver<bool>,
) {
    let pid = winpipe::client_pid(&pipe);
    let path = pid.and_then(verify::process_path);
    let (reader, mut writer) = tokio::io::split(pipe);

    let trusted = match (&path, &shared.owner) {
        (Some(path), Some(owner)) => verify::same_file(path, owner),
        _ => false,
    };
    let (Some(pid), Some(path), true) = (pid, path.clone(), trusted) else {
        log::warn!("refused a client at {path:?}");
        send(
            &mut writer,
            &Event::Error {
                message: "This OpenWire isn't the one the tunnel service was set up for. \
                          Repair the tunnel service in OpenWire's settings."
                    .into(),
            },
        )
        .await;
        return;
    };

    if !send(
        &mut writer,
        &Event::Hello {
            protocol: PROTOCOL,
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    )
    .await
    {
        return;
    }

    let mut requests = read_requests(reader);
    let mut session: Option<Session> = None;

    loop {
        tokio::select! {
            request = requests.recv() => match request {
                Some(Request::Start(request)) => {
                    if session.is_some() {
                        send(&mut writer, &Event::Error { message: "System-wide routing is already on.".into() }).await;
                        continue;
                    }
                    match start(&shared, id, pid, &path, &request).await {
                        Ok(started) => {
                            log::info!(
                                "routing on: interface {}, {} routes, IPv6 {}, kill switch {}",
                                started.index, started.route_count(), started.ipv6, request.kill_switch
                            );
                            let event = Event::Started { interface_index: started.index, ipv6: started.ipv6 };
                            session = Some(started);
                            if !send(&mut writer, &event).await {
                                break;
                            }
                        }
                        Err(message) => {
                            shared.release(id);
                            if !send(&mut writer, &Event::Error { message }).await {
                                break;
                            }
                        }
                    }
                }
                Some(Request::Stop) => {
                    if let Some(running) = session.take() {
                        running.stop().await;
                        shared.release(id);
                        log::info!("routing off");
                    }
                    if !send(&mut writer, &Event::Stopped { reason: None }).await {
                        break;
                    }
                }
                // The app closed the pipe, exited, or sent something broken.
                None => break,
            },
            reason = ended(&mut session) => {
                log::warn!("routing ended: {reason}");
                if let Some(running) = session.take() {
                    running.stop().await;
                }
                shared.release(id);
                if !send(&mut writer, &Event::Stopped { reason: Some(reason) }).await {
                    break;
                }
            }
            _ = stop.changed() => break,
        }
    }

    if let Some(running) = session.take() {
        running.stop().await;
        log::info!("routing off: the app went away");
    }
    shared.release(id);
}

/// Requests off the pipe, read by a task of their own: a read cut short by
/// `select!` would lose its place in the stream.
fn read_requests(reader: tokio::io::ReadHalf<NamedPipeServer>) -> mpsc::Receiver<Request> {
    let (tx, rx) = mpsc::channel(8);
    tokio::spawn(async move {
        let mut reader = BufReader::new(reader);
        let mut line = Vec::new();
        loop {
            line.clear();
            match (&mut reader)
                .take(MAX_LINE)
                .read_until(b'\n', &mut line)
                .await
            {
                Ok(0) | Err(_) => break,
                Ok(_) if line.last() != Some(&b'\n') => {
                    log::warn!("a client sent an oversized request");
                    break;
                }
                Ok(_) => {}
            }
            let Ok(request) = serde_json::from_slice::<Request>(&line) else {
                log::warn!("a client sent a request this service does not know");
                break;
            };
            if tx.send(request).await.is_err() {
                break;
            }
        }
    });
    rx
}

async fn send(writer: &mut WriteHalf<NamedPipeServer>, event: &Event) -> bool {
    let Ok(mut line) = serde_json::to_vec(event) else {
        return false;
    };
    line.push(b'\n');
    writer.write_all(&line).await.is_ok() && writer.flush().await.is_ok()
}

async fn ended(session: &mut Option<Session>) -> String {
    match session {
        Some(session) => session
            .ended
            .recv()
            .await
            .unwrap_or_else(|| "System-wide routing stopped unexpectedly.".into()),
        None => std::future::pending().await,
    }
}

// ---------------------------------------------------------------------------
// A session
// ---------------------------------------------------------------------------

/// Everything one routing session holds. Taken down by [`Session::stop`], in
/// an order that never lets traffic out unprotected.
struct Session {
    routes: Vec<Routes>,
    stop_engine: watch::Sender<bool>,
    engine: Option<JoinHandle<()>>,
    reader: Option<std::thread::JoinHandle<()>>,
    firewall: Option<Firewall>,
    session: Arc<wintun::Session>,
    /// Dropped last: closing it removes the adapter from the system.
    _adapter: Arc<Adapter>,
    ended: mpsc::Receiver<String>,
    index: u32,
    ipv6: bool,
}

impl Drop for Session {
    fn drop(&mut self) {
        // However the session ends - `stop`, or a task torn down mid-way -
        // the reader must be woken: it holds the packet session, and with it
        // the adapter, for as long as it sits waiting. Then the fields drop
        // in declaration order: routes first, the adapter last.
        self.session.shutdown();
        let _ = self.stop_engine.send(true);
    }
}

impl Session {
    fn route_count(&self) -> usize {
        self.routes.iter().map(Routes::len).sum()
    }

    async fn stop(mut self) {
        // Routes first: traffic returns to the real network at once, while
        // the firewall still stands (with the kill switch on, it is blocked
        // there instead of leaking).
        self.routes.clear();

        // Then the data path: wake the reader, end the engine and its flows.
        self.session.shutdown();
        let _ = self.stop_engine.send(true);
        if let Some(engine) = self.engine.take() {
            let _ = engine.await;
        }
        if let Some(reader) = self.reader.take() {
            let _ = tokio::task::spawn_blocking(move || reader.join()).await;
        }

        // The filters last of what shapes traffic. Then the session and the
        // adapter go as `self` drops.
        drop(self.firewall.take());
    }
}

struct Writer(Arc<wintun::Session>);

impl PacketWriter for Writer {
    fn write(&self, packet: &[u8]) {
        self.0.send(packet);
    }
}

fn validate(request: &StartRequest) -> Result<(), String> {
    let refuse = |what: &str| {
        Err(format!(
            "System-wide routing was asked for {what}, so it stayed off."
        ))
    };
    if request.proxy_port == 0 {
        return refuse("no local proxy");
    }
    if request.exclude.len() > MAX_EXCLUDED || request.dns.len() > MAX_DNS {
        return refuse("more addresses than it handles");
    }
    if request
        .exclude
        .iter()
        .any(|ip| ip.is_unspecified() || ip.is_multicast())
    {
        return refuse("an address it cannot route around");
    }
    let adapter_v4 = Cidr::new(INTERFACE_V4.into(), PREFIX_V4);
    if request.dns.iter().any(|ip| {
        ip.is_unspecified()
            || ip.is_loopback()
            || ip.is_multicast()
            || adapter_v4.contains(&Cidr::host(*ip))
    }) {
        return refuse("a DNS server it cannot reach through the tunnel");
    }
    Ok(())
}

async fn start(
    shared: &Shared,
    client: u64,
    pid: u32,
    app: &Path,
    request: &StartRequest,
) -> Result<Session, String> {
    validate(request)?;

    // The port must be the asking app's own: otherwise anything that could
    // open the pipe could aim every app's traffic at a listener of its own.
    let listener = tokio::task::spawn_blocking({
        let port = request.proxy_port;
        move || verify::loopback_listener(port)
    })
    .await
    .map_err(|_| internal())?;
    if listener != Some(pid) {
        return Err(
            "OpenWire's local proxy isn't running, so there is nothing to route to. Reconnect and try again."
                .into(),
        );
    }

    if !shared.claim(client) {
        return Err("System-wide routing is already on for another OpenWire window.".into());
    }
    let wintun = shared.wintun()?;

    let opened = tokio::task::spawn_blocking({
        let request = request.clone();
        let app = app.to_path_buf();
        move || open(&wintun, &request, &app)
    })
    .await
    .map_err(|_| internal())??;

    // The data path, before any route points at it.
    let (packets_tx, packets_rx) = mpsc::channel::<Vec<u8>>(1024);
    let (ended_tx, ended_rx) = mpsc::channel::<String>(2);

    let reader = {
        let session = Arc::clone(&opened.session);
        let ended = ended_tx.clone();
        std::thread::Builder::new()
            .name("tunnel-rx".into())
            .spawn(move || loop {
                match session.receive() {
                    Received::Packet(packet) => {
                        if packets_tx.blocking_send(packet).is_err() {
                            break;
                        }
                    }
                    Received::Shutdown => break,
                    Received::Closed => {
                        let _ = ended.try_send("The OpenWire network adapter was removed.".into());
                        break;
                    }
                }
            })
            .map_err(|err| {
                log::error!("reader thread: {err}");
                internal()
            })?
    };

    let resolvers: Vec<SocketAddr> = if request.dns.is_empty() {
        DEFAULT_DNS.to_vec()
    } else {
        request.dns.clone()
    }
    .into_iter()
    .map(|ip| SocketAddr::new(ip, 53))
    .collect();
    let config = engine::Config {
        proxy: SocketAddr::from(([127, 0, 0, 1], request.proxy_port)),
        resolvers,
        dns_address: SocketAddr::new(IpAddr::V4(DNS_V4), 53),
        local_subnets: vec![
            Cidr::new(INTERFACE_V4.into(), PREFIX_V4),
            Cidr::new(INTERFACE_V6.into(), PREFIX_V6),
        ],
        mtu: MTU,
    };
    let (stop_engine, engine_stopped) = watch::channel(false);
    let stopping = engine_stopped.clone();
    let writer: Arc<dyn PacketWriter> = Arc::new(Writer(Arc::clone(&opened.session)));
    let engine = tokio::spawn(async move {
        let outcome = engine::run(config, packets_rx, writer, engine_stopped).await;
        // Ending without being asked to - with an error or without one -
        // leaves routes pointing at nothing, so the session must go.
        if !*stopping.borrow() {
            if let Err(err) = outcome {
                log::error!("engine: {err}");
            }
            let _ = ended_tx.try_send("System-wide routing stopped unexpectedly.".into());
        }
    });

    let mut session = Session {
        routes: Vec::new(),
        stop_engine,
        engine: Some(engine),
        reader: Some(reader),
        firewall: Some(opened.firewall),
        session: opened.session,
        _adapter: opened.adapter,
        ended: ended_rx,
        index: opened.index,
        ipv6: opened.ipv6,
    };

    // Routes last: only now is everything in place to carry what they send.
    let luid = opened.luid;
    let v4 = cidr::routes_v4(&request.exclude, request.lan_bypass);
    let v6 = session
        .ipv6
        .then(|| cidr::routes_v6(&request.exclude, request.lan_bypass));
    let routes = tokio::task::spawn_blocking(move || {
        let luid = windows_sys::Win32::NetworkManagement::Ndis::NET_LUID_LH { Value: luid };
        let mut added = vec![Routes::add(&luid, &v4)?];
        if let Some(v6) = v6 {
            added.push(Routes::add(&luid, &v6)?);
        }
        Ok::<_, std::io::Error>(added)
    })
    .await
    .map_err(|_| internal())?;

    match routes {
        Ok(routes) => {
            session.routes = routes;
            Ok(session)
        }
        Err(err) => {
            log::error!("routes: {err}");
            session.stop().await;
            Err("Windows wouldn't accept the routes for system-wide routing. Try again.".into())
        }
    }
}

/// What `open` set up, for `start` to finish.
struct Opened {
    adapter: Arc<Adapter>,
    session: Arc<wintun::Session>,
    firewall: Firewall,
    luid: u64,
    index: u32,
    ipv6: bool,
}

/// The adapter, its addresses and DNS, a packet session, and the firewall.
/// Blocking: creating an adapter can take a second or two.
fn open(wintun: &Arc<Wintun>, request: &StartRequest, app: &Path) -> Result<Opened, String> {
    let adapter_failed = |step: &str, err: std::io::Error| {
        log::error!("{step}: {err}");
        "Windows wouldn't set up the OpenWire network adapter. Restarting your PC usually fixes this."
            .to_string()
    };

    let adapter = Arc::new(
        Adapter::create(wintun, ADAPTER_NAME, "OpenWire", ADAPTER_GUID)
            .map_err(|err| adapter_failed("create adapter", err))?,
    );
    if let Some((major, minor)) = wintun.driver_version() {
        log::info!("wintun driver {major}.{minor}");
    }
    let luid = adapter.luid();
    let index = netcfg::interface_index(&luid).map_err(|err| adapter_failed("index", err))?;

    netcfg::prepare(&luid, false, MTU).map_err(|err| adapter_failed("ipv4 interface", err))?;
    netcfg::add_address(&luid, INTERFACE_V4.into(), PREFIX_V4)
        .map_err(|err| adapter_failed("ipv4 address", err))?;
    // IPv6 is best effort: it can be switched off on the machine. Without it
    // here, IPv6 has no route through the adapter - and the kill switch, if
    // on, still blocks it on the real network.
    let ipv6 = match netcfg::prepare(&luid, true, MTU)
        .and_then(|()| netcfg::add_address(&luid, INTERFACE_V6.into(), PREFIX_V6))
    {
        Ok(()) => true,
        Err(err) => {
            log::warn!("no IPv6 on the adapter: {err}");
            false
        }
    };
    netcfg::set_dns(&luid, &[IpAddr::V4(DNS_V4)]).map_err(|err| adapter_failed("dns", err))?;

    let session = Arc::new(
        wintun::Session::start(Arc::clone(&adapter), RING)
            .map_err(|err| adapter_failed("session", err))?,
    );

    let firewall = Firewall::enable(&Rules {
        interface: netcfg::luid_value(&luid),
        app,
        kill_switch: request.kill_switch,
        lan_bypass: request.lan_bypass,
    })
    .map_err(|err| {
        log::error!("{err}");
        "Windows Filtering Platform refused OpenWire's leak protection, so routing stayed off."
            .to_string()
    })?;

    Ok(Opened {
        adapter,
        session,
        firewall,
        luid: netcfg::luid_value(&luid),
        index,
        ipv6,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> StartRequest {
        StartRequest {
            proxy_port: 10808,
            exclude: vec!["203.0.113.10".parse().unwrap()],
            lan_bypass: true,
            kill_switch: false,
            dns: vec![],
        }
    }

    #[test]
    fn sensible_requests_pass_and_odd_ones_are_refused() {
        assert!(validate(&request()).is_ok());

        let mut no_port = request();
        no_port.proxy_port = 0;
        assert!(validate(&no_port).is_err());

        let mut flood = request();
        flood.exclude = vec!["203.0.113.10".parse().unwrap(); MAX_EXCLUDED + 1];
        assert!(validate(&flood).is_err());

        for dns in ["127.0.0.1", "0.0.0.0", "172.19.0.2", "224.0.0.1"] {
            let mut bad = request();
            bad.dns = vec![dns.parse().unwrap()];
            assert!(validate(&bad).is_err(), "{dns} should be refused");
        }
    }

    #[test]
    fn one_session_at_a_time() {
        let shared = Shared {
            owner: None,
            wintun: Mutex::new(None),
            active: Mutex::new(None),
        };
        assert!(shared.claim(1));
        assert!(!shared.claim(2));
        shared.release(2); // not the holder: no effect
        assert!(!shared.claim(2));
        shared.release(1);
        assert!(shared.claim(2));
    }
}
