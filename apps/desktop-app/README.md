# Maizy VPN — desktop client

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
3. **Outbound** (`core/tunnel/vless.rs`) — TCP connects to the VPS, but the TLS
   ClientHello carries the camouflage hostname. The real destination is inside
   the encrypted VLESS payload.
4. **System proxy** (`core/sysproxy/windows.rs`) — per-user WinINET registry
   values, captured before they are touched and restored on disconnect, on exit,
   and on panic.

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
machine-global WinINET settings that a running Maizy session also owns, so a
routine `cargo test` must not switch your networking out from under you.

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
| `security` | Must be `tls`                                                    |

Links are written to `%APPDATA%/com.maizy.vpn/servers.json` on this machine
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

Only `security=tls` over `type=tcp`. Links using REALITY, XTLS, WebSocket, gRPC
or HTTPUpgrade are **rejected at import** with a message saying why, rather than
being added as a node that fails later with a confusing handshake error.

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
| Apps / Hosts / Traffic type, and "Through Maizy" | The relay's own per-connection counters | Only traffic that went through the tunnel |

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
  connections rather than letting traffic out unmasked.
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
    usage/                history tiers, per-app attribution, interface counters
    sysproxy/             per-platform OS proxy control
    catalog · models · metrics · probe · store
  state.rs                the one owner of mutable state
  error.rs · tray.rs · lib.rs
```

## Not built yet

- **TUN/Wintun capture.** Routing is system-proxy based, which covers apps that
  honour the OS proxy. A virtual adapter would catch the rest.
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
