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
  tier: Tier
  load: number
  tags: string[]
  protocol: 'direct' | 'vless-tls'
  /** False until the node has credentials; connecting to it will be refused. */
  configured: boolean
  latencyMs: number | null
}

export interface Settings {
  listenPort: number
  systemProxy: boolean
  killSwitch: boolean
  autoConnect: boolean
  launchMinimized: boolean
  lanBypass: boolean
  monitorWhenIdle: boolean
  preferredServer: string | null
}

export type SettingsPatch = Partial<Settings>

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

export interface UsageEntry {
  key: string
  up: number
  down: number
  total: number
  /** PNG data URL of the app's icon. Apps column only, and only when the
   *  executable could be located and read. */
  icon?: string
}

export interface UsageSummary {
  /** Midday of the first and last local day these figures cover. */
  fromTs: number
  toTs: number
  totalUp: number
  totalDown: number
  tunnelUp: number
  tunnelDown: number
  apps: UsageEntry[]
  hosts: UsageEntry[]
  kinds: UsageEntry[]
  /** False where per-app names cannot be resolved on this platform. */
  attributionSupported: boolean
}

export const EMPTY_SUMMARY: UsageSummary = {
  fromTs: 0,
  toTs: 0,
  totalUp: 0,
  totalDown: 0,
  tunnelUp: 0,
  tunnelDown: 0,
  apps: [],
  hosts: [],
  kinds: [],
  attributionSupported: true,
}
