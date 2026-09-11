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
  preferredServer: string | null
}

export type SettingsPatch = Partial<Settings>

export interface PlatformInfo {
  systemProxySupported: boolean
  platform: string
  version: string
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
