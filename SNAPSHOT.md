# OpenWire — project snapshot

State of the repository as of **2026-09-12**. Written for whoever picks this up
next. Records what works, what is broken, and the things that cost time to
discover so they do not cost it twice.

---

## 1. What this is

A desktop VPN / traffic-camouflage client. A local proxy wraps outbound traffic
in TLS carrying a **decoy SNI**, so deep packet inspection sees a connection to a
whitelisted service rather than to the user's VPS.

```
app  ──SOCKS5/HTTP──▶  127.0.0.1:10808  ──VLESS over TLS──▶  VPS  ──▶  internet
                       (Rust, loopback)   ClientHello SNI =
                                          aka.ms / teams.microsoft.com
```

Monorepo (`turbo` + `bun` workspaces). Only `apps/desktop-app` is populated.

| | |
|---|---|
| Shell | Tauri 2.11, Rust 1.97 |
| Frontend | React 19.2, Vite 8, Tailwind 4, shadcn on Base UI |
| Charts | recharts 3.8 · Motion | motion 13 (`motion/react`) |
| Icons / font | `@hugeicons/react` (stroke-rounded) · Figtree Variable |
| Size | Rust ~5,100 LOC / 26 files · TS ~3,300 LOC / 35 files (excl. vendored shadcn) |

Primary colour is `#14f471` on near-black. Dark-only by design; colour is
reserved for protection state, never decoration.

---

## 2. Status

### Working and verified

| Area | Verified by |
|---|---|
| SOCKS5 + HTTP proxy on one port (first-byte sniff) | unit tests on real sockets |
| VLESS-over-TLS with decoy SNI | live connection to a real VPS |
| Dual-name certificate verification | fixed a real handshake failure, see §5 |
| LAN/loopback bypass routing | table-driven unit test |
| Kill switch (fail closed) | wired; not exercised end to end |
| System proxy engage/restore + **crash recovery** | opt-in registry round-trip tests |
| Per-app attribution (PID → exe name) | test resolves a live loopback socket |
| Monitor mode: attribution with no VPN connected | bundled-node guard test; shares the relay path |
| Device-wide per-app usage via WinRT/SRUM | live test against the real table: 91 apps, 18 GB/30d |
| Usage history tiers + day buckets | unit tests incl. retention and capping |
| System-wide throughput counters | **over-counts — see §3** |
| Tray, custom window chrome, 5 views | ran the built binary |

### Test counts

```
cargo test                 →  56 passed, 0 failed, 6 ignored
cargo test -- --ignored    →   6         (4 registry + interface dump + SRUM report)
bun run build              →  clean
bunx eslint <own files>    →  clean
```

The 19 remaining eslint errors in the repo are all pre-existing, inside the
vendored `src/components/ui/*` shadcn dump. Not ours.

---

## 3. Open — decided but NOT applied

### 3.1 Usage meter over-counts throughput by ~6× ⚠

**Diagnosed, fix identified, awaiting go-ahead. Do not ship the meter until this
lands.**

`GetIfTable2` returns 41 interfaces on the test machine. Six report the
*identical* byte counter — the real adapter plus five **NDIS lightweight filter
drivers** bound to it (WFP hooks, QoS scheduler, Native WiFi). `read_interfaces`
sums all of them.

| Interface | hw | filter | in_octets |
|---|---|---|---|
| Wi-Fi-WFP Native MAC Layer LightWeight Filter | 0 | **1** | 878,320,520 |
| Wi-Fi-Virtual WiFi Filter Driver | 0 | **1** | 878,320,520 |
| Wi-Fi-Native WiFi Filter Driver | 0 | **1** | 878,320,520 |
| Wi-Fi-QoS Packet Scheduler | 0 | **1** | 878,320,520 |
| Wi-Fi-WFP 802.3 MAC Layer LightWeight Filter | 0 | **1** | 878,320,520 |
| **Wi-Fi** | **1** | 0 | 878,320,520 |

Measured against a known 20 MB download: adapter moved **21,140,668 B**, the app
recorded **128,366,694 B** → ratio **6.08**.

Fix in `core/usage/netstat.rs::read_interfaces`:

```rust
let flags = row.InterfaceAndOperStatusFlags._bitfield;
if flags & 0x02 != 0 { continue; }   // FilterInterface mirrors another adapter
```

Prefer `HardwareInterface` (`flags & 0x01`) rows when any exist — that also
prevents the same class of double-count when a VPN tunnel adapter (Cloudflare
WARP) is active, since its bytes would otherwise count on both the tunnel and the
physical NIC. Keep a fallback so a host with no hardware interface does not
silently report zero.

**Rollout note:** existing `usage.json` is inflated and cannot be corrected
retroactively — the multiplier depends on how many filters were bound at the
time. Delete it when the fix lands.

`cargo test diagnose_interfaces -- --ignored --nocapture` dumps the interface
table. That is what found this; keep it.

### 3.3 App icons in the usage breakdown — done

`core/usage/icon.rs` reads the exe's icon with `ExtractIconExW` + `GetDIBits`
and returns a PNG data URL on `UsageEntry.icon`. `ProcessResolver` keeps the
name → full path map that makes this possible, and it is persisted in
`usage.json` as `appPaths` so icons appear after a restart, before the app has
opened a connection again. Icons are cached per exe name, misses included: the
summary is polled every ten seconds and must not re-enter the shell each time.

An exe with no embedded icon (the cargo test runner, for one) yields `None` and
the row simply renders name-only.

### 3.2 Scale picker should use the custom Select — done

Swapped to `components/ui/select.tsx`. `items={SCALE_OPTIONS}` on the root is
what makes the trigger show the label instead of the raw byte-rate value.

---

## 4. Architecture

```
apps/desktop-app/
  src/
    lib/            ipc · types · format · motion tokens
    components/     title-bar · nav-rail · segmented · notice-bar · setting-row · view-shell
    features/
      vpn/          dashboard, power orb, live stats, provider + context
      servers/      location side sheet, vless:// import
      usage/        network-view (throughput graph) + usage-view (totals ring,
                    ranked breakdown) - two nav pages, one data layer
      protection/   kill switch, LAN bypass, routing, startup
      settings/     port, servers, diagnostics, restore-network

  src-tauri/src/
    commands/       vpn · servers · settings · usage       (the IPC surface)
    core/
      tunnel/       inbound (socks5+http) · vless · relay · health watch
      usage/        history tiers · process attribution · interface counters
      sysproxy/     mod (guard + recovery) · windows · unsupported
      catalog · models · metrics · probe · store
    state.rs        sole owner of mutable state; emits every UI event
    error.rs        AppError → { code, message }, human text only
    tray.rs · lib.rs
```

**Design rules actually enforced in the code**

- `state.rs` is the only place mutable state lives. `update_connection` mutates,
  emits `vpn:state`, and syncs the tray in one step — the webview can never
  observe a phase the backend has moved past.
- Errors cross IPC as `{ code, message }`. The `Serialize` impl logs full detail
  and sends only `user_message()`. Raw IO/TLS strings never reach the UI.
- Locks are `std::sync` and never held across `.await`, so tray callbacks can
  read state without an async context.
- The relay hot path is two atomics per buffer. Everything expensive happens on
  the 1 Hz sampler.

### IPC surface

16 commands:

```
vpn_status  vpn_stats  vpn_connect  vpn_disconnect  vpn_toggle
servers_list  servers_probe  servers_import  servers_remove  servers_optimal
settings_get  settings_update  platform_info  network_restore
usage_timeline  usage_summary
```

4 events: `vpn:state` · `vpn:stats` · `vpn:notice` · `usage:sample` (1 Hz, fires
whether or not the tunnel is up).

`vpn_disconnect` and `settings_update` take an `AppHandle` so they can start or
stop monitor mode; the wire signatures the UI sees are unchanged.

### On-disk state — `%APPDATA%/com.openwire.desktop/`

| File | Contents |
|---|---|
| `settings.json` | port, toggles, preferred server |
| `servers.json` | catalog **including credentials** — never leaves the machine |
| `usage.json` | history tiers + per-day breakdowns (90-day retention) |
| `proxy-backup.json` | present only while the system proxy is engaged; see §5.2 |

---

## 5. Hard-won gotchas

### 5.1 Certificate verification must accept *two* names

A masked server proves the name it owns (the endpoint), while the ClientHello
carries the decoy. Verifying only the decoy rejects every self-hosted node:

```
certificate not valid for name "aka.ms";
certificate is only valid for DnsName("meetzoom.mayantha.online")
```

`vless.rs` therefore accepts a cert valid for **either** the endpoint or the SNI.
Both come from the user's own link, so either proof authenticates the intended
peer. Chain, expiry and revocation are still enforced. This is why users do not
need `allowInsecure=1` here, unlike most clients.

### 5.2 `tauri dev` strands the system proxy

The dev server **force-kills** the binary on every rebuild, so `Drop` never runs
and the registry stays pointed at a dead port — every site stops loading. Same
for a crash or Task Manager kill.

Mitigated: the snapshot is written to disk *before* the OS is touched, startup
detects a proxy still aimed at our loopback port and undoes it, and
**Settings → Restore network settings** does it on demand. Recovery only touches
settings still pointing at us, so a user-configured proxy is never clobbered.

### 5.3 Registry tests are `#[ignore]`d on purpose

They mutate machine-global WinINET settings that a *running* OpenWire session also
owns. During development a routine `cargo test` switched off a live session's
proxy. Run deliberately with `cargo test -- --ignored`.

They also share one registry key, so they serialise through `sysproxy::lock_registry()`.
Without it they race and fail non-deterministically.

### 5.4 PowerShell hides what Win32 shows

`Get-NetAdapter` / `Get-NetAdapterStatistics` omit filter pseudo-interfaces, even
with `-IncludeHidden`. That hid §3.1 for a full diagnostic round. When checking
interface behaviour, read the raw `GetIfTable2` table.

### 5.5 Disk pressure on `W:`

The drive runs near full (~3 GB free). `src-tauri/target/` grows to ~8.5 GB and
has twice hit `os error 112` mid-build. `cargo clean` reclaims it; a rebuild is
~4 min. Consider pointing `CARGO_TARGET_DIR` at a roomier drive — **not changed**,
since it affects the whole environment.

### 5.6 Attribution has two sources, and they answer different questions

**Apps: Windows' own accounting (`core/usage/srum.rs`).** Windows already
records per-app network bytes for Settings -> Network -> Data usage, and
`GetAttributedNetworkUsageAsync` hands it to an unpackaged desktop app with no
elevation. That is device-wide per-app attribution for free - 91 apps and 18 GB
over 30 days on the development box. It carries **no destinations**: bytes per
app, nothing about hosts or ports.

Two shapes of attribution id come back: an NT device path
(`\device\harddiskvolume9\...\msedge.exe`), mapped to a DOS path via
`QueryDosDeviceW` so the icon extractor can open it, and a package family name
(`5319275A.WhatsAppDesktop_cv1g1gvanyjgm`), trimmed to a readable leaf. An empty
id is real: it is the kernel and the shared service hosts, shown as
`System and services`.

It is **slow**. A machine keeps a `ConnectionProfile` per network it has ever
joined - 42 here - and each is its own round-trip. Starting them all before
awaiting any takes a 30-day query from ~15s to ~7s, and no further. So it is
fetched per period, cached by day span, and run on a blocking worker. Never poll
it, and never put it on the 1 Hz sampler.

**Hosts and traffic type: the relay only.** Nothing names a host unless the
bytes passed through our listener. Windows exposes no per-process byte counters
to an unelevated process - `GetExtendedTcpTable` gives PID and remote address
but no counters, `GetPerTcpConnectionEStats` needs Administrator,
`GetProcessIoCounters` mixes disk with network, and `GetIfTable2` is per-adapter
only. An ETW kernel trace would give both halves at once, but needs
Administrator or *Performance Log Users*.

**Monitor mode** widens what the relay sees, for the columns SRUM cannot fill:
`monitorWhenIdle` keeps the `openwire-direct` route up while no VPN is connected,
so the system proxy still points at us and the same per-connection accounting
runs with the outbound hop missing. `Phase::Monitoring` exists to keep that
visibly distinct from `Connected` - the orb stays off and the headline says the
VPN is off, because nothing is masked. `isRouting()` on the TS side is the "does
the proxy belong to us" question that `isActive()` must not be used for, or the
restore-network button will fight a live monitor session.

Do not resolve an "empty columns" report by making the ring feed them, and do
not try to reconcile Apps against Hosts: they are measured by different systems
over the same days, and only Apps is device-wide.

### 5.7 Tooling

- Large Rust files with lifetimes/quotes break bash heredocs. Use the Write tool.
- `HugeiconsProps['icon']` is optional; the correct prop type is `IconSvgElement`.
- recharts is installed but was **transitive only** until pinned in
  `package.json`. The vendored `components/ui/chart.tsx` expects it.
- Two commits (`21c283d`, `689c74f`) were created by something in the
  environment, not by hand. Worth knowing before reading the history.
- `cargo test` needs ~2 GB of headroom on `W:` because the `staticlib`
  crate-type writes a ~920 MB `app_lib.lib` on every build (mobile targets only;
  desktop needs just cdylib + rlib). `cargo clean -p app` frees it without
  rebuilding dependencies - about 30 s, against ~4 min for a full `cargo clean`.

---

## 6. Deliberately not built

| | Why |
|---|---|
| TUN / Wintun capture | Routing is system-proxy based. A virtual adapter would catch apps that ignore the OS proxy. |
| Remote catalog, accounts, tiers | `DOCS/desktop-app.md` describes a Next.js + Supabase control plane. Explicitly deferred; `servers.json` is the source of truth. Premium is a badge with nothing behind it. |
| macOS / Linux system proxy | Tunnel runs everywhere; automatic OS routing is Windows-only. UI says so and shows the manual address. |
| Host names for traffic that never reaches the proxy | Windows records which app used the network but not where it connected, so hosts and traffic type need a WFP driver or an ETW kernel trace (Administrator). Per-app totals do NOT need either - see 5.6. |
| uTLS fingerprint mimicry (`fp=chrome`) | Parsed, not applied. rustls has its own JA3. Matters only against handshake-fingerprinting DPI, not SNI filtering. |
| Host geolocation | Needs a GeoIP DB, which is not shipped. |
| REALITY / XTLS / WebSocket / gRPC | Rejected at import with a reason, rather than failing later with a cryptic handshake error. |

---

## 7. Running it

```bash
bun install
cd apps/desktop-app
bun run tauri:dev        # NOTE: strands the system proxy on each reload — see §5.2
bun run tauri:build

cd src-tauri
cargo test               # 56, safe
cargo test -- --ignored  # 6, touches the real registry and reads SRUM
```

**Adding a server** — paste a `vless://` share link in Settings → Your servers,
or the location sheet. Only `security=tls` over `type=tcp` is supported.

**Nothing ships pre-added.** The catalog starts empty; every server comes from
a pasted share link. Earlier builds seeded six placeholder locations with empty
credentials - `catalog::strip_unusable` removes any unconfigured node on load,
and a `preferredServer` pointing at something no longer present is cleared, so
existing installs lose the dead rows without losing a server the user added.

**`Direct (diagnostic)`** (`catalog::DIRECT_NODE`) is a node with no remote:
the proxy dials destinations itself. It is **not** in the user's catalog and
never appears in the server list - it exists only to back monitor mode, which
resolves it by id. It does not mask traffic.
