# OpenWire — desktop client

Tauri 2 + React 19 + Rust. A local proxy that wraps outbound traffic in TLS with
a camouflaged SNI, so deep packet inspection sees a connection to a whitelisted
service rather than to your VPS.

## How traffic actually flows

```
app  ──SOCKS5/HTTP──▶  127.0.0.1:10808  ──VLESS over TLS──▶  VPS  ──▶  internet
                       (Rust, loopback)   ClientHello SNI =
                                          teams.microsoft.com
```

1. **Inbound** (`core/tunnel/inbound.rs`) — one loopback listener speaks both
   SOCKS5 and HTTP proxy, chosen by sniffing the first byte. Windows apps mostly
   use the HTTP proxy; everything else prefers SOCKS5.
2. **Routing** (`core/tunnel/mod.rs`) — LAN and loopback destinations bypass the
   tunnel when *Bypass local network* is on; everything else goes out masked.
3. **Outbound** (`core/tunnel/vless.rs`, `trojan.rs`) — TCP connects to the VPS,
   but the TLS ClientHello carries the camouflage hostname. The real destination
   is inside the encrypted VLESS payload, framed by the link's transport
   (`core/tunnel/transport/`): raw, WebSocket, gRPC, XHTTP, HTTPUpgrade or HTTP/2.
4. **System proxy** (`core/sysproxy/windows.rs`) — per-user WinINET registry
   values, captured before they are touched and restored on disconnect, on exit,
   and on panic.

### Routing every app (TUN)

The system proxy only reaches apps that follow it. With **Settings → Routing →
Route every app** on, a virtual network adapter carries everything instead -
games, command-line tools, UDP, and every DNS lookup:

```
every app ─▶ Wintun adapter ─▶ OpenWire Tunnel service ─▶ 127.0.0.1:10808 ─▶ VPS
             (routes, DNS)     (userspace TCP/IP stack)   (the same proxy)
```

1. **Service** (`core/tun/service.rs`) — adapters, routes and firewall rules
   need administrator rights, so a small Windows service does them, installed
   by the setup program (or from Settings, with one UAC prompt). The app stays
   unelevated and keeps the server, its credentials and the protocols; the
   service never sees them. This is the split WireGuard, Mullvad and Tailscale
   use. The service shares `core/winsvc` with the network monitor.
2. **Control pipe** (`core/tun/client.rs`) — the app asks over
   `\\.\pipe\openwire-tunnel`. The service only answers the OpenWire
   executable it was installed from, and only routes to a proxy port that very
   process is listening on (`core/tun/verify.rs`). A session lasts exactly as
   long as the app holds the pipe open.
3. **Adapter** (`core/tun/wintun.rs`, `netcfg.rs`) — WireGuard's signed Wintun
   driver, bound by hand and loaded by full path. The adapter gets
   `172.19.0.1/30` and `fdfe:dcba:9876::1/126`, the lowest interface metric,
   and DNS server `172.19.0.2`.
4. **Routes** (`core/tun/cidr.rs`) — everything except the server's own
   addresses, loopback, link-local and multicast (and private ranges with the
   LAN bypass) is routed into the adapter, as `/1`-or-narrower routes cut
   around those holes. What is left out follows the machine's own default
   route, so roaming needs no update.
5. **Stack** (`core/tun/engine.rs`) — smoltcp, through `netstack-smoltcp`,
   terminates each TCP connection; UDP arrives as datagrams. Each becomes a
   flow on the local proxy through a private SOCKS5 method
   (`core/tun/wire.rs`), which carries the app's own source address for usage
   attribution. UDP rides VLESS's UDP command or Trojan's UDP ASSOCIATE. QUIC
   (UDP 443) is dropped so browsers use TCP, which travels a TCP tunnel better.
6. **DNS** (`core/tun/dns.rs`) — every query to port 53, to any address, is
   answered through the tunnel by `1.1.1.1` / `8.8.8.8` over pipelined TCP,
   with IDs rewritten so two apps cannot receive each other's answers.
7. **No loops** (`core/net.rs`) — the app's own sockets must not follow the
   routes back into itself. The server's name is resolved once before routing
   starts and pinned, and every socket the app opens is bound to the real
   network's interface with `IP_UNICAST_IF`.

## Running it

```bash
bun install
bun run tauri:dev      # from apps/desktop-app
bun run tauri:build    # installer
```

Rust tests (protocol framing, relay, LAN routing, usage accounting):

```bash
cd src-tauri && cargo test
cd src-tauri && cargo test -- --ignored   # OS proxy tests, see below
```

The system-proxy tests are `#[ignore]`d on purpose: they mutate the
machine-global WinINET settings that a running OpenWire session also owns, so a
routine `cargo test` must not switch your networking out from under you.

Frontend unit tests: `bun test tests`.

### Screenshots

The README screenshots come from the real UI running on made-up data
(`screenshots.html` → `src/demo/`), so no real address or device appears in
them. To regenerate them (needs Node and Chrome or Edge):

```bash
bun scripts/screenshots.ts   # writes docs/screenshots/*.webp
```

## Adding a server

The bundled catalog ships with **no credentials** — remote nodes show as *Setup
required* until you add your own. Paste a `vless://` share link in **Settings →
Your servers** or in the location picker:

```
vless://<uuid>@<vps-host>:443?encryption=none&security=tls&sni=teams.microsoft.com&type=tcp#Tokyo
```

| Parameter  | Meaning                                                         |
| ---------- | --------------------------------------------------------------- |
| `<uuid>`   | Your access key from the server config                           |
| `<vps-host>` | Where the TCP connection really goes                          |
| `sni`      | Name shown to the network — the camouflage                       |
| `security` | `tls`, or `none` for a server without TLS                         |
| `type`     | `tcp`, `ws`, `grpc`, `xhttp`, `httpupgrade` or `http` (HTTP/2)   |

Links are written to `%APPDATA%/com.openwire.desktop/servers.json` on this machine
only. They are never committed, bundled, or sent anywhere — and the UI never
receives the key back from the backend, only the host.

**`Direct (diagnostic)`** is a bundled node with no remote at all: the proxy
dials destinations itself. Use it to confirm the listener, system proxy, and
throughput counters work before blaming a server. It does **not** mask traffic,
and the UI says so while it is in use.

### Server side

Any VLESS + TLS server works (Xray, V2Ray). Minimum inbound:

```json
{
  "protocol": "vless",
  "port": 443,
  "settings": {
    "clients": [{ "id": "<uuid>" }],
    "decryption": "none"
  },
  "streamSettings": {
    "network": "tcp",
    "security": "tls",
    "tlsSettings": { "certificates": [{ "certificateFile": "...", "keyFile": "..." }] }
  }
}
```

### Certificate checking with a decoy SNI

Certificate validation is **on** by default, and it accounts for the fact that
the ClientHello name is a decoy. A self-hosted node can only ever prove the name
it owns, so the client accepts a certificate valid for **either**:

- the endpoint you dial (`meetzoom.example.org`) — the self-hosted case, or
- the masking domain (`sni=`) — the genuinely CDN-fronted case.

Both names come from your own link, so either proof authenticates the peer you
meant to reach. Chain, expiry and revocation are still enforced — only the name
matched against differs. Verifying *only* the decoy would reject every
self-hosted server, which is what `allowInsecure=1` is usually a workaround for.

`allowInsecure=1` still exists and skips the name check entirely. You should not
need it for a normal masked setup.

### Supported subset

`security=tls` or `security=none`, over any of these transports:

| `type=` | Notes |
| --- | --- |
| `tcp` / `raw` | Also `headerType=http`, the fake HTTP request/response head |
| `ws` | Early data via `?ed=` in the path, sent in `Sec-WebSocket-Protocol` |
| `grpc` | `serviceName`, `authority`, gun or `mode=multi`; Xray's `/custom/path` form |
| `xhttp` / `splithttp` | packet-up, stream-up and stream-one, over HTTP/2 or HTTP/1.1; `extra` is read |
| `httpupgrade` | |
| `http` / `h2` | HTTP/2 `PUT`, as v2fly and sing-box serve it (Xray has dropped it) |

The HTTP/2 transports share one connection per session. Links using REALITY,
XTLS Vision (`flow=`), VLESS encryption, mKCP, QUIC, or XHTTP settings this
client doesn't follow (a separate download server, custom placements) are
**rejected at import** with a message saying why, rather than being added as a
node that fails later with a confusing handshake error.

Every transport is tested end to end against real Xray and sing-box servers:

```bash
cd src-tauri && OPENWIRE_XRAY=path/to/xray.exe OPENWIRE_SINGBOX=path/to/sing-box.exe \
  cargo test interop -- --ignored --nocapture
```

`fp=chrome` (uTLS fingerprint mimicry) is parsed but not applied — the client
uses rustls, whose TLS fingerprint differs from Chrome's. This matters only
against DPI that fingerprints the handshake itself; SNI-based filtering is
unaffected.

## Network usage meter

A GlassWire-style meter lives under **Network**, with two tabs.

**Graph** — live throughput at one sample per second, across five time ranges
(5 min / 3 hours / 24 hours / week / month), a fixed-or-auto y-axis ceiling, a
pause control on the live range, and a scrubber on the rolled-up ones.

**Usage** — a totals ring plus three ranked columns: Apps, Hosts, Traffic type.

Two different data sources feed it, and the difference matters:

| Shown | Source | Scope |
| --- | --- | --- |
| Graph, and the ring's Total/Incoming/Outgoing | `GetIfTable2` interface counters | Everything this machine sends and receives |
| Apps / Hosts / Traffic type, and "Through OpenWire" | The relay's own per-connection counters | Only traffic that went through the tunnel |

Per-app names come from the OS TCP table: apps reach the proxy over loopback, so
the client-side port identifies the owning process (`GetExtendedTcpTable` →
`QueryFullProcessImageNameW`). No kernel driver and no elevation — which is the
whole reason this works where GlassWire needs both. Naming the app behind
arbitrary *system* traffic does need a driver, so those columns stay tunnel-only
and the UI says so rather than looking broken.

History is stored in three tiers (seconds → minutes → hours) plus per-day
breakdowns, so a month costs a few thousand rows rather than two million. Daily
buckets use the machine's local midnight. Retention is 90 days; per-day key maps
are capped, with the tail folded into `Other` so a long browsing session cannot
grow the store without bound. It lives in `usage.json` beside the other config.

## Safety properties

- The listener binds `127.0.0.1` only and rejects non-loopback peers, so it can
  never become an open relay for your network.
- System proxy changes are snapshotted and restored by an RAII guard — quitting,
  crashing, or disconnecting all put your settings back.
- The snapshot is also written to disk **before** the OS is touched, so a
  process that is *killed* rather than asked to stop still leaves a recoverable
  trail. The next launch detects a proxy still aimed at our loopback port and
  undoes it; **Settings → Restore network settings** does the same on demand.
  Recovery only touches settings still pointing at us, so a proxy you configured
  yourself is never clobbered.
- **Kill switch** fails closed: while the server is unreachable the proxy refuses
  connections rather than letting traffic out unmasked. With every app routed
  through the adapter it also installs Windows Filtering Platform rules that
  block anything leaving outside the adapter - except the app itself, DHCP,
  IPv6 neighbour discovery, and the local network with the LAN bypass.
- With every app routed, DNS outside the adapter is blocked whatever the kill
  switch says: Windows asks every network's resolver in parallel, so being
  first in line is not enough.
- Nothing system-wide routing changes can outlive it. The routes and DNS
  server live on the adapter, which Windows removes with the process that
  created it; the firewall rules are in a dynamic WFP session, deleted when
  its handle closes. The service takes everything down the moment the app's
  pipe closes - an app crash restores the network rather than stranding it.
- The webview has no filesystem, shell, or HTTP capability (see
  `capabilities/default.json`), and runs under a restrictive CSP.
- Errors cross the IPC boundary as `{ code, message }` with human-written text;
  raw IO and TLS strings stay in the log.

## Layout

```
src/
  lib/                    ipc, shared types, formatting, motion tokens
  components/             shell chrome (title bar, rail, notices, setting rows)
  features/
    vpn/                  dashboard, power orb, live stats
    servers/              location picker, vless:// import
    usage/                throughput graph, totals ring, ranked breakdown
    protection/           kill switch, LAN bypass, routing, startup
    settings/             port, servers, diagnostics

src-tauri/src/
  commands/               the IPC surface (vpn, servers, settings)
  core/
    tunnel/               inbound proxy, VLESS outbound, relay, health watch
    tun/                  system-wide routing: service, Wintun, routes, stack, DNS
    winsvc/               install, elevation and pipes shared by both services
    netmon/               the network monitor service
    usage/                history tiers, per-app attribution, interface counters
    sysproxy/             per-platform OS proxy control
    net.rs                the app's own sockets, kept off the adapter
    catalog · models · metrics · probe · store
  state.rs                the one owner of mutable state
  error.rs · tray.rs · lib.rs
```

## Not built yet

- **TUN on macOS and Linux.** `core/tun/engine.rs` is platform-free; it needs
  a utun or `/dev/net/tun` device and a privileged helper on each.
- **IPv6 DNS servers on the adapter, and ICMP.** Queries over IPv4 already
  carry AAAA lookups; ping through the tunnel is not something VLESS or Trojan
  can carry, so pings time out rather than lie.
- **Remote catalog + accounts.** `DOCS/desktop-app.md` describes a Next.js and
  Supabase control plane; the client is deliberately standalone for now, and
  `servers.json` is the source of truth.
- **macOS / Linux system proxy.** The tunnel runs everywhere; automatic OS
  routing is Windows-only, and the UI says so and shows the manual address.
- **Per-app names for non-tunnel traffic.** Attribution covers connections that
  went through the proxy. System-wide per-app accounting needs a WFP/ETW
  driver; totals for that traffic are shown, names are not.
- **App icons and host geolocation** in the usage columns: both need assets the
  app does not ship (icon extraction, a GeoIP database).
