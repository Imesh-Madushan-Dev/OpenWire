# Project Overview: Desktop Proxy Tunneling & VPN Client (Tauri + Rust + React)

## 1. Introduction
This project focuses on the design and architectural implementation of a cross-platform Desktop VPN and Traffic Camouflage Client. Built specifically for desktop environments (Windows, macOS, and Linux), the application allows users to bypass network-level inspection and bandwidth throttles by camouflaging outbound traffic as authorized or zero-rated communication services (such as Zoom, Microsoft Teams, or specific cloud hosts). 

By establishing an encrypted proxy tunnel directly from the desktop operating system to dedicated remote Virtual Private Servers (VPS), users achieve private, unrestricted, and high-performance network access.

---

## 2. Core System Architecture
The desktop system is structured into three primary architectural tiers:

### 2.1 Front-End User Interface (React)
- **Role:** Handles the graphical interface, interaction states, real-time metrics display (ping latency, upload/download speeds, connection status), server location switching, and subscription tier validation.
- **Characteristics:** Highly responsive, lightweight, and hardware-accelerated via native web view integration provided by Tauri (WebView2 on Windows, WebKit on macOS/Linux), eliminating the heavy memory overhead typical of Chromium-based solutions.

### 2.2 Desktop System Core (Rust & Tauri Backend)
- **Role:** Serves as the native system-level bridge that controls operating system networking, system tray integration, service lifecycle, authentication tokens, and underlying tunnel binaries/libraries.
- **Privilege & System Integration:** Communicates with OS network adapters (such as virtual TUN devices or system proxy hooks), configures routing tables, and manages local background listeners. Rust guarantees high execution speed and memory safety with zero garbage collection pauses.

### 2.3 Remote Server Infrastructure & Control Plane
- **Cloud Server Nodes (e.g., Linode, DigitalOcean):** Host routing and decapsulation engines running specialized protocols (such as VLESS / Shadowsocks with TLS masking).
- **Central Management & Control API (Next.js & Supabase):** Serves as the single source of truth for user authentication, server availability, node health, routing configurations, and subscription tier access levels. (DOMT IMPLIMENT THIS THIS IS LATER INTERGRATION)

---

## 3. Network Tunneling and Masking Mechanism

The desktop implementation handles network traffic through specialized low-level tunneling and deep packet camouflage:

### 3.1 Packet Interception & Virtual Interface
1. **Network Layer Redirection:** The client leverages a virtual network adapter (such as a Wintun or TUN interface) or local loopback system proxy. Outbound traffic is systematically routed into the local Rust process.
2. **Encapsulation:** The intercepted IP packets or TCP/UDP streams are encapsulated into secure protocol payloads.

### 3.2 SNI Manipulation & TLS Handshake Masking
1. **Header Modification:** When initiating outbound connections to the remote VPS, the client wraps the data stream within a standard TLS handshake.
2. **SNI Spoofing:** In the `ClientHello` phase of the TLS handshake, the Server Name Indication (SNI) header is dynamically populated with the domain of a whitelisted or zero-rated platform (e.g., legitimate meeting/conferencing endpoints).
3. **ISP Level Inspection:** To intermediate Deep Packet Inspection (DPI) firewalls and ISP filters, the connection strictly matches legitimate traffic heading toward the whitelisted service.

### 3.3 Remote Ingestion & Decapsulation
1. **VPS Ingestion:** The inbound packet arrives at the remote VPS node, where the server-side process inspects the incoming handshake, terminates the camouflage layer, and authenticates the client session.
2. **Egress Routing:** The original destination request (e.g., a target website or streaming server) is decapsulated and dispatched out to the open internet via the VPS's public network interface.
3. **Reverse Flow:** Inbound response packets are re-encrypted, masked using matching session attributes, and returned through the secure channel to the desktop client.

---

## 4. Key Functional Features & Output

- **One-Click Secure Connection:** Intuitive toggle interface allowing users to establish or terminate the masked tunnel in seconds.
- **Dynamic Server Topology:** Node lists and connection parameters are fetched on startup from the central API, allowing administrators to add or decommission server nodes without app updates.
- **Tier-Based Access Enforcement:**
  - *Standard Tier:* Connects to shared community nodes with bandwidth optimization.
  - *Premium Tier:* Unlocks low-latency, dedicated routing nodes with multi-region selection.
- **Fail-Safe & Diagnostics:** Real-time ping testing to calculate server response times, automatic reconnection logic during network dropouts, and clean teardown of network routing rules to prevent traffic leaks upon unexpected application closure.
- **System Tray Operations:** Background execution with minimal resource footprint, enabling persistent connectivity without cluttering the desktop workspace.