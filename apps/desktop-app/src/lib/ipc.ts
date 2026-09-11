/**
 * The only place the app talks to Rust.
 *
 * Every rejection from Tauri arrives as `{ code, message }` where `message` is
 * already written for a person - see `src-tauri/src/error.rs`. This module
 * normalises that into a real `Error` so callers never surface a raw string or
 * a `[object Object]`.
 */

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import type {
  ConnectionState,
  Notice,
  PlatformInfo,
  ServerSummary,
  Settings,
  SettingsPatch,
  StatsSnapshot,
} from './types'

export class VpnError extends Error {
  readonly code: string

  constructor(code: string, message: string) {
    super(message)
    this.name = 'VpnError'
    this.code = code
  }
}

const FALLBACK_MESSAGE = 'Something went wrong. Please try again.'

function toVpnError(raw: unknown): VpnError {
  if (
    typeof raw === 'object' &&
    raw !== null &&
    'code' in raw &&
    'message' in raw &&
    typeof (raw as { message: unknown }).message === 'string'
  ) {
    const { code, message } = raw as { code: unknown; message: string }
    return new VpnError(typeof code === 'string' ? code : 'UNKNOWN', message)
  }

  // An unshaped rejection means a bug on our side, not something the user can
  // act on - log the detail and show generic text.
  console.error('unexpected IPC rejection', raw)
  return new VpnError('UNKNOWN', FALLBACK_MESSAGE)
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args)
  } catch (raw) {
    throw toVpnError(raw)
  }
}

// -- commands ---------------------------------------------------------------

export const api = {
  status: () => call<ConnectionState>('vpn_status'),
  stats: () => call<StatsSnapshot>('vpn_stats'),
  connect: (serverId?: string | null) =>
    call<ConnectionState>('vpn_connect', { serverId: serverId ?? null }),
  disconnect: () => call<ConnectionState>('vpn_disconnect'),

  servers: () => call<ServerSummary[]>('servers_list'),
  probeServers: () => call<ServerSummary[]>('servers_probe'),
  importServer: (uri: string) => call<ServerSummary>('servers_import', { uri }),
  removeServer: (id: string) => call<void>('servers_remove', { id }),
  optimalServer: () => call<ServerSummary | null>('servers_optimal'),

  settings: () => call<Settings>('settings_get'),
  updateSettings: (patch: SettingsPatch) => call<Settings>('settings_update', { patch }),
  platform: () => call<PlatformInfo>('platform_info'),
}

// -- events -----------------------------------------------------------------

export const EVENTS = {
  state: 'vpn:state',
  stats: 'vpn:stats',
  notice: 'vpn:notice',
} as const

export function onConnectionState(handler: (state: ConnectionState) => void) {
  return listen<ConnectionState>(EVENTS.state, (event) => handler(event.payload))
}

export function onStats(handler: (stats: StatsSnapshot) => void) {
  return listen<StatsSnapshot>(EVENTS.stats, (event) => handler(event.payload))
}

export function onNotice(handler: (notice: Notice) => void) {
  return listen<Notice>(EVENTS.notice, (event) => handler(event.payload))
}

export type { UnlistenFn }
