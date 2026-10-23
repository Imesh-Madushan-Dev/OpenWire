<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="apps/desktop-app/public/logo-dark.svg">
  <img src="apps/desktop-app/public/logo-light.svg" alt="OpenWire logo" width="88" height="88">
</picture>

# OpenWire

**Network monitor and self-hosted tunnel for Windows.**

[![Latest release](https://img.shields.io/github/v/release/Imesh-Madushan-Dev/OpenWire?label=release&color=6d5cff)](https://github.com/Imesh-Madushan-Dev/OpenWire/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/Imesh-Madushan-Dev/OpenWire/total?color=2ea44f)](https://github.com/Imesh-Madushan-Dev/OpenWire/releases)
[![License](https://img.shields.io/github/license/Imesh-Madushan-Dev/OpenWire?color=blue)](LICENSE)
![Windows 10 | 11](https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4)

[**Download**](https://github.com/Imesh-Madushan-Dev/OpenWire/releases/latest) · [Website](https://openwire.iconnix.lk) · [Developer guide](apps/desktop-app/README.md) · [Report a bug](https://github.com/Imesh-Madushan-Dev/OpenWire/issues)

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/vpn-dark.webp">
  <img src="docs/screenshots/vpn.webp" alt="OpenWire's VPN page: tunnel on, connected to Singapore, with a list of servers and their latency" width="820">
</picture>

</div>

## Overview

OpenWire shows where your PC's traffic goes - by app, host, country and device - and routes it through a VLESS or Trojan server you run yourself. It is free, open source, and has no accounts or telemetry. Everything it records stays on your machine.

## Features

**Tunnel**
- VLESS and Trojan, over TCP, WebSocket, gRPC, XHTTP, HTTPUpgrade or HTTP/2, with a TLS server name (SNI) you choose
- Import from a `vless://` or `trojan://` link, a subscription, a file or a QR code image; share as a link or QR code
- Live latency per server, with automatic selection of the fastest
- Kill switch, local network bypass, and your previous proxy settings restored on disconnect, exit or crash

**Monitoring**
- Live graph of all traffic, from the last 5 minutes to the last month
- Usage by app, host (with country) and traffic type, for any day, week or month
- Optional detailed monitoring of every app, from Windows' own network events - no kernel driver, and no packet contents read
- Upload and download speed on the taskbar

**Local network**
- Device scan with name, maker and type
- Alerts when a new device joins

**App**
- Three themes in light and dark, with five chart palettes
- Update notifications, checked against GitHub's releases

<table>
  <tr>
    <td width="50%">
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/network-dark.webp">
        <img src="docs/screenshots/network.webp" alt="Network page: a live graph of download and upload">
      </picture>
    </td>
    <td width="50%">
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/usage-dark.webp">
        <img src="docs/screenshots/usage.webp" alt="Usage page: data used by app, host and traffic type">
      </picture>
    </td>
  </tr>
  <tr>
    <td width="50%">
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/devices-dark.webp">
        <img src="docs/screenshots/devices.webp" alt="Devices page: devices on the local network with their maker and type">
      </picture>
    </td>
    <td width="50%">
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/settings-dark.webp">
        <img src="docs/screenshots/settings.webp" alt="Appearance settings: colour themes and chart palettes">
      </picture>
    </td>
  </tr>
</table>

## Getting started

1. Download `OpenWire_x.y.z_x64-setup.exe` from [Releases](https://github.com/Imesh-Madushan-Dev/OpenWire/releases/latest) and run it. Administrator approval is needed once.
2. Copy your server's share link and press **Ctrl+V** on the VPN page, or choose **Add**.
3. Press the power button to connect.

Requires Windows 10 or 11 (64-bit). The installer adds the WebView2 runtime if it is missing. Releases are not code-signed yet, so SmartScreen may ask you to confirm - each release lists SHA-256 checksums to verify against.

### Supported servers

| | Supported |
| --- | --- |
| **Protocols** | VLESS, Trojan |
| **Transports** | TCP (plain or with an HTTP header), WebSocket, gRPC, XHTTP, HTTPUpgrade, HTTP/2 |
| **Security** | TLS, or none |
| **Tested against** | Xray-core, sing-box |

Links using REALITY, XTLS Vision, VLESS encryption, VMess, mKCP or QUIC are refused at import, with the reason.

## How it works

```
your apps ──SOCKS5/HTTP──▶ OpenWire (127.0.0.1:10808) ──VLESS/Trojan + transport──▶ your server ──▶ internet
```

OpenWire runs a local SOCKS5 and HTTP proxy, bound to loopback only, and points the Windows system proxy at it while connected. Local network destinations can skip the tunnel; everything else is sent to your server inside the link's transport, wrapped in TLS. See the [developer guide](apps/desktop-app/README.md) for certificate checking, the usage meter's data sources and the safety guarantees.

## Privacy

No accounts, analytics or telemetry. Apart from your own servers, OpenWire only talks to devices on your local network when it scans them, and to GitHub to check for a new release - which you can turn off in **Settings > General**.

| Data | Where it stays |
| --- | --- |
| Servers, settings, usage history, devices | `%APPDATA%\com.openwire.desktop` |
| IP-to-country lookups | A database bundled with the app |
| Detailed monitoring | Read locally from Windows' network events; traffic contents are never read |

## Development

Requires [Bun](https://bun.sh) 1.2+, [Rust](https://rustup.rs) (stable, MSVC) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/).

```bash
git clone https://github.com/Imesh-Madushan-Dev/OpenWire.git
cd OpenWire && bun install
cd apps/desktop-app

bun run tauri:dev       # run with hot reload
bun run tauri:build     # installers in src-tauri/target/release/bundle
```

Before opening a pull request:

```bash
bunx tsc -b && bun test tests && bun run lint
cd src-tauri && cargo test && cargo clippy
```

Architecture, tests and the screenshot generator are covered in the [developer guide](apps/desktop-app/README.md). For larger changes, please open an issue first.

## Roadmap

- [ ] Firewall - allow or block apps from reaching the network
- [ ] TUN mode, for apps that ignore the system proxy
- [ ] REALITY and VMess
- [ ] Automatic routing on macOS and Linux

## Acknowledgements

IP geolocation by [DB-IP](https://db-ip.com), licensed under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). MAC vendors from the [IEEE Registration Authority](https://standards-oui.ieee.org/). Built with [Tauri](https://tauri.app), [React](https://react.dev), [Recharts](https://recharts.org), [Motion](https://motion.dev) and [Hugeicons](https://hugeicons.com). Layout inspired by [GlassWire](https://www.glasswire.com).

## License

[GPL-3.0](LICENSE) © 2026 Imesh Madushan. OpenWire is for routing your own traffic through servers you control; you are responsible for using it in line with local laws and the terms of the networks you use.
