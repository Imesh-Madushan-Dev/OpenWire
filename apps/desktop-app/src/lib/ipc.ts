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
  DeviceKind,
  DevicesSnapshot,
  ImportReport,
  MonitorStatus,
  Notice,
  PlatformInfo,
  Ranked,
  ScanProgress,
  ServerEdit,
  ServerSummary,
  Settings,
  SettingsPatch,
  SpanTimeline,
  StatsSnapshot,
  UsageRange,
  UsageSample,
  UsageSpan,
  UsageSummary,
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
  /** Every link in a block of text, or in a base64 subscription body. */
  importText: (text: string) => call<ImportReport>('servers_import_text', { text }),
  /** Read in the backend, so the link never passes through the webview. */
  importClipboard: () => call<ImportReport>('servers_import_clipboard'),
  /** The server's link, credentials included. Only for an explicit Share. */
  shareLink: (id: string) => call<string>('servers_share_link', { id }),
  updateServer: (id: string, edit: ServerEdit) =>
    call<ServerSummary>('servers_update', { id, edit }),
  removeServer: (id: string) => call<void>('servers_remove', { id }),
  optimalServer: () => call<ServerSummary | null>('servers_optimal'),

  usageTimeline: (range: UsageRange) => call<UsageSample[]>('usage_timeline', { range }),
  /** Throughput between two instants, at the finest tier that covers the start. */
  usageTimelineBetween: (fromTs: number, toTs: number) =>
    call<SpanTimeline>('usage_timeline_between', { fromTs, toTs }),
  usageSummary: (range: UsageRange) => call<UsageSummary>('usage_summary', { range }),
  usageSummaryBetween: (fromTs: number, toTs: number) =>
    call<UsageSummary>('usage_summary_between', { fromTs, toTs }),
  usageSpan: () => call<UsageSpan>('usage_span'),
  /** Device-wide per-app usage from Windows. Slow (seconds) - never poll this. */
  usageDeviceApps: (fromTs: number, toTs: number) =>
    call<Ranked>('usage_device_apps', { fromTs, toTs }),
  /** Icons for app names, as data URLs; apps without one are absent. */
  usageIcons: (apps: string[]) => call<Record<string, string>>('usage_icons', { apps }),

  netmonStatus: () => call<MonitorStatus>('netmon_status'),
  /** Shows the Windows admin prompt; resolves once it has been answered. */
  netmonEnable: () => call<MonitorStatus>('netmon_enable'),
  netmonDisable: () => call<MonitorStatus>('netmon_disable'),

  devices: () => call<DevicesSnapshot>('devices_snapshot'),
  /** Sweeps the local network; resolves when it has finished. */
  scanDevices: () => call<DevicesSnapshot>('devices_scan'),
  renameDevice: (mac: string, name: string | null) =>
    call<DevicesSnapshot>('devices_rename', { mac, name }),
  /** `null` returns the device to the automatic guess. */
  setDeviceKind: (mac: string, kind: DeviceKind | null) =>
    call<DevicesSnapshot>('devices_set_kind', { mac, kind }),
  /** `null` marks every device as seen. */
  acknowledgeDevices: (mac: string | null) =>
    call<DevicesSnapshot>('devices_acknowledge', { mac }),
  forgetDevice: (mac: string) => call<DevicesSnapshot>('devices_forget', { mac }),

  settings: () => call<Settings>('settings_get'),
  updateSettings: (patch: SettingsPatch) => call<Settings>('settings_update', { patch }),
  platform: () => call<PlatformInfo>('platform_info'),

  restoreNetwork: () => call<string>('network_restore'),
}

// -- events -----------------------------------------------------------------

export const EVENTS = {
  state: 'vpn:state',
  stats: 'vpn:stats',
  notice: 'vpn:notice',
  usageSample: 'usage:sample',
  netmonStatus: 'netmon:status',
  devices: 'devices:changed',
  devicesProgress: 'devices:progress',
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

/** One tick per second, emitted whether or not the tunnel is up. */
export function onUsageSample(handler: (sample: UsageSample) => void) {
  return listen<UsageSample>(EVENTS.usageSample, (event) => handler(event.payload))
}

export function onMonitorStatus(handler: (status: MonitorStatus) => void) {
  return listen<MonitorStatus>(EVENTS.netmonStatus, (event) => handler(event.payload))
}

export function onDevices(handler: (snapshot: DevicesSnapshot) => void) {
  return listen<DevicesSnapshot>(EVENTS.devices, (event) => handler(event.payload))
}

/** Addresses asked so far in a sweep, whichever view started it. */
export function onDevicesProgress(handler: (progress: ScanProgress) => void) {
  return listen<ScanProgress>(EVENTS.devicesProgress, (event) => handler(event.payload))
}

export type { UnlistenFn }
