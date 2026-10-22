/**
 * Mirrors `src-tauri/src/core/models.rs`. The Rust side serialises camelCase,
 * so these shapes map one-to-one with no translation layer.
 */

export type Phase =
  | 'disconnected'
  | 'connecting'
  | 'connected'
  | 'reconnecting'
  | 'disconnecting'
  /** Routing locally so usage can be attributed, with nothing masked. */
  | 'monitoring'

export type Tier = 'free' | 'premium'

export interface ConnectionState {
  phase: Phase
  serverId: string | null
  serverName: string | null
  countryCode: string | null
  /** Endpoint traffic exits from; `null` while disconnected. */
  exitHost: string | null
  /** Loopback address apps should point at, e.g. `127.0.0.1:10808`. */
  listenAddr: string | null
  connectedAt: number | null
  systemProxy: boolean
  note: string | null
}

export interface StatsSnapshot {
  upBytes: number
  downBytes: number
  upBps: number
  downBps: number
  activeConnections: number
  totalConnections: number
  latencyMs: number | null
  uptimeSecs: number
}

export interface ServerSummary {
  id: string
  name: string
  country: string
  countryCode: string
  city: string
  host: string
  port: number
  tier: Tier
  load: number
  tags: string[]
  protocol: 'direct' | 'vless-tls' | 'trojan-tls'
  /** The masking domain sent in the TLS handshake; empty for direct. */
  sni: string
  allowInsecure: boolean
  /** False until the node has credentials; connecting to it will be refused. */
  configured: boolean
  latencyMs: number | null
}

/** What can be changed about a server after adding it. Credentials are not
 *  here: a new key means importing the server's new link. */
export interface ServerEdit {
  name?: string
  host?: string
  port?: number
  sni?: string
  allowInsecure?: boolean
}

export interface ImportReport {
  imported: ServerSummary[]
  /** Links found but not imported - unsupported protocols, mostly. */
  failed: number
  firstError: string | null
}

export interface Settings {
  listenPort: number
  systemProxy: boolean
  killSwitch: boolean
  autoConnect: boolean
  launchMinimized: boolean
  lanBypass: boolean
  monitorWhenIdle: boolean
  /** Sweep the local network every few minutes and say when a device joins. */
  watchDevices: boolean
  /** Live upload and download speed on the Windows taskbar. */
  taskbarSpeed: boolean
  preferredServer: string | null
}

export type SettingsPatch = Partial<Settings>

// -- devices on the local network ---------------------------------------------

export type DeviceKind =
  | 'router'
  | 'computer'
  | 'phone'
  | 'tablet'
  | 'tv'
  | 'speaker'
  | 'console'
  | 'printer'
  | 'camera'
  | 'storage'
  | 'smartHome'
  | 'wearable'
  | 'network'
  | 'unknown'

export type DeviceRole = 'router' | 'thisDevice'

/** How a device's own name was learnt. */
export type NameSource = 'mdns' | 'netbios' | 'dns'

export interface Device {
  mac: string
  ip: string
  /** What to call it: the user's name for it, else its own. */
  name: string | null
  hostname: string | null
  nameSource: NameSource | null
  customName: string | null
  vendor: string | null
  /** A randomised address, so the maker cannot be known. */
  privateAddress: boolean
  kind: DeviceKind
  kindIsCustom: boolean
  role: DeviceRole | null
  online: boolean
  /** Unix seconds. */
  firstSeen: number
  lastSeen: number
  isNew: boolean
}

export interface NetworkInfo {
  id: string
  /** What Windows calls the adapter, e.g. "Wi-Fi". */
  adapter: string
  wireless: boolean
  subnet: string
  address: string
  gateway: string
}

export interface DevicesSnapshot {
  supported: boolean
  /** `null` with no local network to scan: offline, or only a VPN or cellular link. */
  network: NetworkInfo | null
  devices: Device[]
  scanning: boolean
  lastScan: number | null
}

export interface ScanProgress {
  done: number
  total: number
}

export interface PlatformInfo {
  systemProxySupported: boolean
  platform: string
  version: string
}

/** The period the store holds, for bounding the date pickers. */
export interface UsageSpan {
  /** Midday of the first and last recorded day; `null` when nothing is stored. */
  firstTs: number | null
  lastTs: number | null
  retentionDays: number
}

export interface Notice {
  level: 'info' | 'warning' | 'error'
  message: string
}

/** A line of the connection log - mirrors `core/connlog.rs`. */
export interface LogEntry {
  /** Increasing; merges a fetched list with live events. */
  id: number
  /** Unix milliseconds. */
  ts: number
  level: 'info' | 'success' | 'warning' | 'error'
  message: string
  /** The first line of a connection attempt. */
  sessionStart: boolean
}

export const INITIAL_CONNECTION: ConnectionState = {
  phase: 'disconnected',
  serverId: null,
  serverName: null,
  countryCode: null,
  exitHost: null,
  listenAddr: null,
  connectedAt: null,
  systemProxy: false,
  note: null,
}

export const INITIAL_STATS: StatsSnapshot = {
  upBytes: 0,
  downBytes: 0,
  upBps: 0,
  downBps: 0,
  activeConnections: 0,
  totalConnections: 0,
  latencyMs: null,
  uptimeSecs: 0,
}

/** True while the tunnel is up, including a degraded/reconnecting session. */
export function isActive(phase: Phase): boolean {
  return phase === 'connected' || phase === 'reconnecting'
}

/** True while a transition is in flight and the toggle should be inert. */
export function isBusy(phase: Phase): boolean {
  return phase === 'connecting' || phase === 'disconnecting'
}

/**
 * True whenever the local proxy holds the OS routing - protected or not.
 *
 * Distinct from `isActive` on purpose: monitor mode owns the system proxy
 * without protecting anything, so "is the user safe" and "is the proxy in use"
 * are different questions and the wrong one silently breaks the other.
 */
export function isRouting(phase: Phase): boolean {
  return isActive(phase) || phase === 'monitoring'
}

// ---------------------------------------------------------------------------
// Network usage — mirrors `src-tauri/src/core/usage/`
// ---------------------------------------------------------------------------

/** Which tier of history a query reads from. */
export type UsageRange = 'fiveMinutes' | 'threeHours' | 'day' | 'week' | 'month'

export interface UsageSample {
  /** Unix seconds at the start of the bucket. */
  t: number
  /** Everything across this machine's interfaces, in bytes. */
  up: number
  down: number
  /** The subset that went through the tunnel. */
  tunnelUp: number
  tunnelDown: number
}

/** A timeline for a chosen span, with the resolution it came at. */
export interface SpanTimeline {
  /** Seconds each sample covers: 60 for a span inside the last day, else 3600. */
  bucketSecs: number
  samples: UsageSample[]
}

export interface UsageEntry {
  key: string
  up: number
  down: number
  total: number
  /** ISO country code, on hosts only, when the address it was reached at is known. */
  country?: string
}

/** The tail past the listed entries, summed: "+41 more, 189 MB". */
export interface UsageMore {
  count: number
  up: number
  down: number
}

/** One ranked column, largest first, with `Other` always last. */
export interface Ranked {
  entries: UsageEntry[]
  more: UsageMore | null
}

export interface UsageColumns {
  apps: Ranked
  hosts: Ranked
  kinds: Ranked
}

export interface UsageSummary {
  /** Midday of the first and last local day these figures cover. */
  fromTs: number
  toTs: number
  totalUp: number
  totalDown: number
  tunnelUp: number
  tunnelDown: number
  /** Only what went through OpenWire's proxy. */
  openwire: UsageColumns
  /** Everything on the device, from the network monitor with OpenWire's own
   *  figures folded in. `null` when the monitor recorded nothing here. */
  device: UsageColumns | null
  /** Midday of the first day in the period the monitor has figures for. */
  monitoredFromTs: number | null
  /** False where per-app names cannot be resolved on this platform. */
  attributionSupported: boolean
}

export const EMPTY_RANKED: Ranked = { entries: [], more: null }

const EMPTY_COLUMNS: UsageColumns = {
  apps: EMPTY_RANKED,
  hosts: EMPTY_RANKED,
  kinds: EMPTY_RANKED,
}

export const EMPTY_SUMMARY: UsageSummary = {
  fromTs: 0,
  toTs: 0,
  totalUp: 0,
  totalDown: 0,
  tunnelUp: 0,
  tunnelDown: 0,
  openwire: EMPTY_COLUMNS,
  device: null,
  monitoredFromTs: null,
  attributionSupported: true,
}

// ---------------------------------------------------------------------------
// Network monitor — mirrors `src-tauri/src/core/netmon/client.rs`
// ---------------------------------------------------------------------------

export interface MonitorStatus {
  /** False off Windows, where there is nothing to install. */
  supported: boolean
  installed: boolean
  running: boolean
  /** The app is receiving the service's stream right now. */
  connected: boolean
  /** The service's trace session is up. */
  collecting: boolean
  /** Why it is not collecting, already written for a person. */
  error: string | null
  /**
   * Installed by an older OpenWire and needs reinstalling: it speaks another
   * protocol, or still records but lacks something newer (host countries).
   */
  outdated: boolean
}
