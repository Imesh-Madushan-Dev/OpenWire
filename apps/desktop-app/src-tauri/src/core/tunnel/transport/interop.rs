//! Every transport against real servers, end to end through the tunnel:
//! SOCKS5 in, the share link's framing out, an echo server behind.
//!
//! Needs an Xray binary, so it only runs when asked. Xray dropped the h2
//! transport, so that one runs against sing-box, when there is one too:
//!
//! ```text
//! OPENWIRE_XRAY=C:\path\to\xray.exe OPENWIRE_SINGBOX=C:\path\to\sing-box.exe \
//!     cargo test interop -- --ignored --nocapture
//! ```
//!
//! `OPENWIRE_INTEROP_ONLY=ws` narrows it to the cases whose name has `ws`.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::core::catalog;
use crate::core::metrics::Metrics;
use crate::core::probe;
use crate::core::tunnel::{Tunnel, TunnelConfig};
use crate::core::usage::UsageTracker;

const UUID: &str = "2b4f8c1e-0000-4000-8000-abcdefabcdef";
const PASSWORD: &str = "p@ss:word";
const FIRST_PORT: u16 = 24100;
/// Per connection, each way: enough to cross several HTTP/2 windows and
/// split into many upload POSTs.
const PAYLOAD: usize = 384 * 1024;
const CONNECTIONS: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Implementation {
    Xray,
    SingBox,
}

struct Case {
    server: Implementation,
    name: &'static str,
    protocol: &'static str,
    /// Xray's `streamSettings`, or sing-box's `transport`, minus TLS.
    stream: Value,
    tls: bool,
    /// The share link's query, minus security.
    query: &'static str,
}

fn cases() -> Vec<Case> {
    let case = |name, protocol, tls, stream: Value, query| Case {
        server: Implementation::Xray,
        name,
        protocol,
        stream,
        tls,
        query,
    };
    let sing_box = |name, protocol, tls, stream: Value, query| Case {
        server: Implementation::SingBox,
        name,
        protocol,
        stream,
        tls,
        query,
    };
    let xhttp = json!({ "network": "xhttp", "xhttpSettings": { "path": "/xh", "mode": "auto" } });
    vec![
        case(
            "vless tcp tls",
            "vless",
            true,
            json!({ "network": "raw" }),
            "type=tcp",
        ),
        case(
            "vless tcp plain",
            "vless",
            false,
            json!({ "network": "raw" }),
            "type=tcp",
        ),
        case(
            "trojan tcp tls",
            "trojan",
            true,
            json!({ "network": "raw" }),
            "type=tcp",
        ),
        case(
            "vless tcp+http plain",
            "vless",
            false,
            json!({ "network": "raw", "rawSettings": { "header": { "type": "http", "request": { "path": ["/p"] } } } }),
            "type=tcp&headerType=http&host=www.example.com&path=%2Fp",
        ),
        case(
            "vless ws tls early data",
            "vless",
            true,
            json!({ "network": "ws", "wsSettings": { "path": "/ws" } }),
            "type=ws&path=%2Fws%3Fed%3D2048&host=localhost",
        ),
        case(
            "vless ws plain",
            "vless",
            false,
            json!({ "network": "ws", "wsSettings": { "path": "/ws" } }),
            "type=ws&path=%2Fws",
        ),
        case(
            "trojan ws tls",
            "trojan",
            true,
            json!({ "network": "ws", "wsSettings": { "path": "/ws" } }),
            "type=ws&path=%2Fws",
        ),
        case(
            "vless httpupgrade tls",
            "vless",
            true,
            json!({ "network": "httpupgrade", "httpupgradeSettings": { "path": "/up" } }),
            "type=httpupgrade&path=%2Fup",
        ),
        case(
            "trojan httpupgrade plain",
            "trojan",
            false,
            json!({ "network": "httpupgrade", "httpupgradeSettings": { "path": "/up" } }),
            "type=httpupgrade&path=%2Fup",
        ),
        case(
            "vless grpc tls",
            "vless",
            true,
            json!({ "network": "grpc", "grpcSettings": { "serviceName": "svc" } }),
            "type=grpc&serviceName=svc&mode=gun",
        ),
        case(
            "vless grpc multi tls",
            "vless",
            true,
            json!({ "network": "grpc", "grpcSettings": { "serviceName": "svc" } }),
            "type=grpc&serviceName=svc&mode=multi",
        ),
        case(
            "trojan grpc plain (h2c)",
            "trojan",
            false,
            json!({ "network": "grpc", "grpcSettings": { "serviceName": "svc" } }),
            "type=grpc&serviceName=svc",
        ),
        case(
            "vless xhttp h2 packet-up",
            "vless",
            true,
            xhttp.clone(),
            "type=xhttp&path=%2Fxh&mode=packet-up",
        ),
        case(
            "vless xhttp h2 stream-up",
            "vless",
            true,
            xhttp.clone(),
            "type=xhttp&path=%2Fxh&mode=stream-up",
        ),
        case(
            "vless xhttp h2 stream-one",
            "vless",
            true,
            xhttp.clone(),
            "type=xhttp&path=%2Fxh&mode=stream-one",
        ),
        case(
            "vless xhttp h1 tls packet-up",
            "vless",
            true,
            xhttp.clone(),
            "type=xhttp&path=%2Fxh&alpn=http%2F1.1",
        ),
        case(
            "vless xhttp h1 tls stream-one",
            "vless",
            true,
            xhttp.clone(),
            "type=xhttp&path=%2Fxh&alpn=http%2F1.1&mode=stream-one",
        ),
        case(
            "trojan xhttp h1 plain packet-up",
            "trojan",
            false,
            xhttp.clone(),
            "type=xhttp&path=%2Fxh",
        ),
        case(
            "vless xhttp h1 plain stream-up",
            "vless",
            false,
            xhttp.clone(),
            "type=xhttp&path=%2Fxh&mode=stream-up",
        ),
        case(
            "vless xhttp padding range",
            "vless",
            true,
            json!({ "network": "xhttp", "xhttpSettings": { "path": "/xh", "xPaddingBytes": "20-40" } }),
            "type=xhttp&path=%2Fxh&extra=%7B%22xPaddingBytes%22%3A%2220-40%22%7D",
        ),
        sing_box(
            "sing-box vless h2 tls",
            "vless",
            true,
            json!({ "type": "http", "path": "/h2", "host": ["a.example"] }),
            "type=http&path=%2Fh2&host=a.example",
        ),
        sing_box(
            "sing-box trojan h2 tls",
            "trojan",
            true,
            json!({ "type": "http", "path": "/h2" }),
            "type=http&path=%2Fh2",
        ),
        sing_box(
            "sing-box vless ws tls early data",
            "vless",
            true,
            json!({ "type": "ws", "path": "/ws", "max_early_data": 2048,
                    "early_data_header_name": "Sec-WebSocket-Protocol" }),
            "type=ws&path=%2Fws%3Fed%3D2048",
        ),
        sing_box(
            "sing-box vless ws plain",
            "vless",
            false,
            json!({ "type": "ws", "path": "/ws" }),
            "type=ws&path=%2Fws",
        ),
        sing_box(
            "sing-box vless grpc tls",
            "vless",
            true,
            json!({ "type": "grpc", "service_name": "svc" }),
            "type=grpc&serviceName=svc",
        ),
        sing_box(
            "sing-box trojan httpupgrade tls",
            "trojan",
            true,
            json!({ "type": "httpupgrade", "path": "/up" }),
            "type=httpupgrade&path=%2Fup",
        ),
    ]
}

/// Kills the server when the test ends, however it ends.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

fn start_xray(xray: &Path, dir: &Path) -> Server {
    let cert = dir.join("cert");
    let status = Command::new(xray)
        .args(["tls", "cert", "--domain=localhost", "--expire=24h"])
        .arg(format!("--file={}", cert.display()))
        .stdout(Stdio::null())
        .status()
        .expect("run xray tls cert");
    assert!(status.success(), "xray could not make a certificate");

    let inbounds: Vec<Value> = cases()
        .iter()
        .enumerate()
        .filter(|(_, case)| case.server == Implementation::Xray)
        .map(|(i, case)| {
            let mut stream = case.stream.clone();
            if case.tls {
                stream["security"] = json!("tls");
                stream["tlsSettings"] = json!({ "certificates": [{
                    "certificateFile": dir.join("cert.crt"),
                    "keyFile": dir.join("cert.key"),
                }] });
            }
            let settings = match case.protocol {
                "vless" => json!({ "clients": [{ "id": UUID }], "decryption": "none" }),
                _ => json!({ "clients": [{ "password": PASSWORD }] }),
            };
            json!({
                "listen": "127.0.0.1",
                "port": FIRST_PORT + i as u16,
                "protocol": case.protocol,
                "settings": settings,
                "streamSettings": stream,
            })
        })
        .collect();
    let config = json!({
        "log": { "loglevel": "warning" },
        "inbounds": inbounds,
        // Newer Xray refuses private destinations unless told otherwise.
        "outbounds": [{ "protocol": "freedom", "settings": {
            "finalRules": [{ "action": "allow", "ip": ["127.0.0.0/8"] }]
        } }],
    });
    run_server(xray, dir, "xray", &config)
}

/// Uses the certificate [`start_xray`] made.
fn start_sing_box(sing_box: &Path, dir: &Path) -> Server {
    let inbounds: Vec<Value> = cases()
        .iter()
        .enumerate()
        .filter(|(_, case)| case.server == Implementation::SingBox)
        .map(|(i, case)| {
            let users = match case.protocol {
                "vless" => json!([{ "uuid": UUID }]),
                _ => json!([{ "password": PASSWORD }]),
            };
            let mut inbound = json!({
                "type": case.protocol,
                "listen": "127.0.0.1",
                "listen_port": FIRST_PORT + i as u16,
                "users": users,
                "transport": case.stream,
            });
            if case.tls {
                inbound["tls"] = json!({
                    "enabled": true,
                    "alpn": ["h2", "http/1.1"],
                    "certificate_path": dir.join("cert.crt"),
                    "key_path": dir.join("cert.key"),
                });
            }
            inbound
        })
        .collect();
    let config = json!({
        "log": { "level": "warn" },
        "inbounds": inbounds,
        "outbounds": [{ "type": "direct" }],
    });
    run_server(sing_box, dir, "sing-box", &config)
}

fn run_server(binary: &Path, dir: &Path, name: &str, config: &Value) -> Server {
    let config_path = dir.join(format!("{name}.json"));
    std::fs::write(&config_path, serde_json::to_vec_pretty(config).unwrap()).unwrap();

    let log = std::fs::File::create(dir.join(format!("{name}.log"))).unwrap();
    let child = Command::new(binary)
        .arg("run")
        .arg("-c")
        .arg(&config_path)
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .unwrap_or_else(|e| panic!("start {name}: {e}"));
    Server(child)
}

async fn echo_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let (mut r, mut w) = stream.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });
    port
}

/// One connection through the tunnel: SOCKS5 to the echo server, then
/// `PAYLOAD` bytes out and the same bytes back.
async fn round_trip(proxy: std::net::SocketAddr, echo_port: u16, seed: u8) -> Result<(), String> {
    let mut client = TcpStream::connect(proxy).await.map_err(|e| e.to_string())?;
    client
        .write_all(&[5, 1, 0])
        .await
        .map_err(|e| e.to_string())?;
    let mut ack = [0u8; 2];
    client
        .read_exact(&mut ack)
        .await
        .map_err(|e| e.to_string())?;
    let mut connect = vec![5, 1, 0, 1, 127, 0, 0, 1];
    connect.extend_from_slice(&echo_port.to_be_bytes());
    client
        .write_all(&connect)
        .await
        .map_err(|e| e.to_string())?;
    let mut reply = [0u8; 10];
    client
        .read_exact(&mut reply)
        .await
        .map_err(|e| e.to_string())?;
    if reply[1] != 0 {
        return Err(format!(
            "tunnel refused the connection (SOCKS reply {})",
            reply[1]
        ));
    }

    let payload: Vec<u8> = (0..PAYLOAD)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect();
    let (mut read, mut write) = client.into_split();
    let sent = payload.clone();
    let writer = tokio::spawn(async move {
        // Uneven writes, so framing boundaries land everywhere.
        for piece in sent.chunks(7_919) {
            write.write_all(piece).await?;
        }
        Ok::<_, std::io::Error>(write)
    });
    let mut echoed = vec![0u8; PAYLOAD];
    read.read_exact(&mut echoed)
        .await
        .map_err(|e| format!("read back: {e}"))?;
    let _write = writer
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    if echoed != payload {
        return Err("echoed bytes differ".into());
    }
    Ok(())
}

async fn run_case(index: usize, case: &Case, echo_port: u16) -> Result<(), String> {
    let (scheme, secret) = match case.protocol {
        "vless" => ("vless", UUID.to_string()),
        _ => ("trojan", "p%40ss%3Aword".to_string()),
    };
    let security = if case.tls {
        "security=tls&sni=localhost&allowInsecure=1"
    } else {
        "security=none"
    };
    let link = format!(
        "{scheme}://{secret}@127.0.0.1:{}?{security}&{}#case",
        FIRST_PORT + index as u16,
        case.query
    );
    let node = catalog::parse_share_link(&link).map_err(|e| format!("import: {e}"))?;
    if probe::latency(&probe::Target::of(&node)).await.is_none() {
        return Err("the latency probe got no answer".into());
    }

    let (events, _events_rx) = mpsc::unbounded_channel();
    let tunnel = Tunnel::start(
        TunnelConfig {
            listen_port: 0,
            node,
            lan_bypass: false,
            kill_switch: false,
        },
        Arc::new(Metrics::new()),
        Arc::new(UsageTracker::load(Path::new("no-such-usage.json"))),
        events,
    )
    .await
    .map_err(|e| format!("start: {e}"))?;

    // Concurrently, so the pooled HTTP/2 connection carries several streams.
    let runs = (0..CONNECTIONS).map(|seed| round_trip(tunnel.local_addr, echo_port, seed as u8));
    let results = timeout(Duration::from_secs(30), futures_join_all(runs.collect()))
        .await
        .map_err(|_| "timed out".to_string())?;
    tunnel.stop();
    results.into_iter().collect()
}

async fn futures_join_all<F: std::future::Future + Send + 'static>(
    futures: Vec<F>,
) -> Vec<F::Output>
where
    F::Output: Send + 'static,
{
    let handles: Vec<_> = futures.into_iter().map(tokio::spawn).collect();
    let mut out = Vec::with_capacity(handles.len());
    for handle in handles {
        out.push(handle.await.expect("round trip panicked"));
    }
    out
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs an Xray binary in OPENWIRE_XRAY"]
async fn every_transport_interoperates_with_real_servers() {
    let Some(xray) = std::env::var_os("OPENWIRE_XRAY").map(PathBuf::from) else {
        eprintln!("OPENWIRE_XRAY is not set; skipping");
        return;
    };
    let sing_box = std::env::var_os("OPENWIRE_SINGBOX").map(PathBuf::from);
    let dir = std::env::temp_dir().join(format!("openwire-interop-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _xray = start_xray(&xray, &dir);
    let _sing_box = sing_box.as_deref().map(|path| start_sing_box(path, &dir));
    tokio::time::sleep(Duration::from_secs(2)).await;
    let echo_port = echo_server().await;

    let only = std::env::var("OPENWIRE_INTEROP_ONLY").unwrap_or_default();
    let mut failed = Vec::new();
    for (index, case) in cases().iter().enumerate() {
        if !case.name.contains(only.as_str())
            || (case.server == Implementation::SingBox && sing_box.is_none())
        {
            continue;
        }
        let started = std::time::Instant::now();
        match run_case(index, case, echo_port).await {
            Ok(()) => eprintln!(
                "ok    {:<34} {:>5} ms",
                case.name,
                started.elapsed().as_millis()
            ),
            Err(err) => {
                eprintln!("FAIL  {:<34} {err}", case.name);
                failed.push(case.name);
            }
        }
    }
    assert!(
        failed.is_empty(),
        "failed: {failed:?} (server logs in {})",
        dir.display()
    );
}
