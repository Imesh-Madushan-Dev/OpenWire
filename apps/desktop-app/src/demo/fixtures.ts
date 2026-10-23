/**
 * Made-up data for the README screenshots (scripts/screenshots.ts).
 *
 * Server addresses come from the documentation ranges (RFC 5737), the local
 * network is a private range, and every name is generic - nothing here
 * describes a real person, machine or network.
 */

import type {
  ConnectionState,
  Device,
  DevicesSnapshot,
  LogEntry,
  MonitorStatus,
  PlatformInfo,
  Ranked,
  ServerSummary,
  Settings,
  SpanTimeline,
  StatsSnapshot,
  UpdateStatus,
  UsageColumns,
  UsageRange,
  UsageSample,
  UsageSpan,
  UsageSummary,
} from '@/lib/types'

const DAY = 86_400
const now = () => Math.floor(Date.now() / 1000)

/** Seeded noise, so every run draws the same screenshots. */
function noise(seed: number) {
  let state = seed >>> 0
  return () => {
    state = (Math.imul(state, 1_664_525) + 1_013_904_223) >>> 0
    return state / 2 ** 32
  }
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

const server = (
  id: string,
  name: string,
  country: string,
  countryCode: string,
  host: string,
  protocol: ServerSummary['protocol'],
  latencyMs: number,
  transport: ServerSummary['transport'] = 'tcp',
): ServerSummary => ({
  id,
  name,
  country,
  countryCode,
  city: name,
  host,
  port: 443,
  tier: 'free',
  load: 0,
  tags: [],
  protocol,
  transport,
  tls: true,
  sni: 'cdn.example.com',
  allowInsecure: false,
  configured: true,
  latencyMs,
})

export const servers: ServerSummary[] = [
  server('singapore', 'Singapore', 'Singapore', 'SG', '198.51.100.17', 'vless', 41),
  server('tokyo', 'Tokyo', 'Japan', 'JP', '203.0.113.24', 'vless', 68, 'ws'),
  server('frankfurt', 'Frankfurt', 'Germany', 'DE', '203.0.113.88', 'trojan', 172),
  server('amsterdam', 'Amsterdam', 'Netherlands', 'NL', '198.51.100.62', 'vless', 179, 'xhttp'),
  server('london', 'London', 'United Kingdom', 'GB', '198.51.100.9', 'trojan', 188, 'grpc'),
  server('new-york', 'New York', 'United States', 'US', '203.0.113.140', 'vless', 236),
]

export const connected = (): ConnectionState => ({
  phase: 'connected',
  serverId: 'singapore',
  serverName: 'Singapore',
  countryCode: 'SG',
  exitHost: '198.51.100.17',
  listenAddr: '127.0.0.1:10808',
  connectedAt: now() - 2 * 3600 - 17 * 60,
  systemProxy: true,
  note: null,
})

export function stats(tick: number): StatsSnapshot {
  const downBps = 2_150_000 + smooth(tick / 3) * 600_000
  const upBps = 190_000 + smooth(tick / 2 + 50) * 90_000
  return {
    upBytes: 214_000_000 + tick * upBps,
    downBytes: 1_870_000_000 + tick * downBps,
    upBps,
    downBps,
    activeConnections: 23,
    totalConnections: 1_408,
    latencyMs: 41,
    uptimeSecs: 2 * 3600 + 17 * 60 + tick,
  }
}

export const settings: Settings = {
  listenPort: 10808,
  systemProxy: true,
  killSwitch: true,
  autoConnect: true,
  launchMinimized: false,
  lanBypass: true,
  monitorWhenIdle: false,
  watchDevices: true,
  taskbarSpeed: true,
  checkUpdates: true,
  preferredServer: 'singapore',
}

export const platform: PlatformInfo = {
  systemProxySupported: true,
  platform: 'windows',
  version: '0.3.0',
}

/** Up to date, unless the demo is asked to show `latest` as a newer release. */
export function update(latest: string | null): UpdateStatus {
  return {
    current: platform.version,
    latest: latest ?? platform.version,
    available: latest !== null,
    checkedAt: now() - 40 * 60,
  }
}

export const monitor: MonitorStatus = {
  supported: true,
  installed: true,
  running: true,
  connected: true,
  collecting: true,
  error: null,
  outdated: false,
}

/** An earlier session that hit trouble, then the current one. */
export function logs(): LogEntry[] {
  const start = (now() - 2 * 3600 - 17 * 60) * 1000
  const lines: [offsetMs: number, LogEntry['level'], string, boolean?][] = [
    [-640_000, 'info', 'Connecting to Tokyo', true],
    [-639_900, 'info', 'OpenWire 0.3.0 on Windows 11 Pro 25H2 (build 26200), 64-bit'],
    [-639_850, 'info', 'Network: Wi-Fi, 192.168.1.20/24, gateway 192.168.1.1, DNS 192.168.1.1'],
    [-639_800, 'info', 'Server: Tokyo - VLESS over TLS to 203.0.113.24:443, SNI cdn.example.com'],
    [-639_700, 'info', 'Local proxy listening on 127.0.0.1:10808 (SOCKS5 and HTTP)'],
    [-639_650, 'info', 'Kill switch on, local network bypass on'],
    [-639_600, 'info', 'System proxy set to 127.0.0.1:10808'],
    [-639_550, 'success', 'Connected to Tokyo'],
    [-636_500, 'warning', 'Server did not answer - traffic may not get through'],
    [-560_000, 'warning', 'Server unreachable. Kill switch is blocking traffic.'],
    [-12_000, 'info', 'Disconnecting'],
    [-11_900, 'info', 'Disconnected - system proxy restored'],
    [0, 'info', 'Connecting to Singapore', true],
    [80, 'info', 'OpenWire 0.3.0 on Windows 11 Pro 25H2 (build 26200), 64-bit'],
    [120, 'info', 'Network: Wi-Fi, 192.168.1.20/24, gateway 192.168.1.1, DNS 192.168.1.1'],
    [160, 'info', 'Server: Singapore - VLESS over TLS to 198.51.100.17:443, SNI cdn.example.com'],
    [240, 'info', 'Local proxy listening on 127.0.0.1:10808 (SOCKS5 and HTTP)'],
    [260, 'info', 'Kill switch on, local network bypass on'],
    [310, 'info', 'System proxy set to 127.0.0.1:10808'],
    [330, 'success', 'Connected to Singapore'],
    [420, 'success', 'Server answered in 41 ms'],
  ]
  return lines.map(([offset, level, message, sessionStart = false], index) => ({
    id: index + 1,
    ts: start + offset,
    level,
    message,
    sessionStart,
  }))
}

// ---------------------------------------------------------------------------
// Traffic
// ---------------------------------------------------------------------------

/** Smooth noise in 0..1: random knots at whole `x`, eased between. */
function smooth(x: number): number {
  const knot = (k: number) => noise(Math.floor(k) * 7_919 + 104_729)()
  const f = x - Math.floor(x)
  const ease = (1 - Math.cos(f * Math.PI)) / 2
  return knot(x) * (1 - ease) + knot(x + 1) * ease
}

/**
 * Bytes per second down and up at a moment: a browsing baseline that wanders,
 * with heavier stretches - a stream buffering, a download - rising out of it.
 */
function rateAt(t: number): [number, number] {
  const heavy = Math.max(0, smooth(t / 11) - 0.45) / 0.55
  const down = 140_000 + smooth(t / 2.5) * 420_000 + heavy ** 2 * 4_800_000
  const up = 30_000 + smooth(t / 3 + 500) * 90_000 + down * 0.05
  return [down, up]
}

/** How busy each hour is, so day and week views have a day's rhythm. */
function hourly(t: number): number {
  const hour = new Date(t * 1000).getHours()
  return hour < 7 ? 0.2 : hour < 12 ? 0.7 : hour < 18 ? 0.9 : 1.3
}

const sample = (t: number, step: number): UsageSample => {
  let down = 0
  let up = 0
  // A wide bucket averages a few moments instead of summing every second.
  const probes = Math.min(step, 12)
  for (let i = 0; i < probes; i++) {
    const [d, u] = rateAt(t + Math.floor((i * step) / probes))
    down += d
    up += u
  }
  const scale = (step >= 60 ? hourly(t) : 1) * (step / probes)
  down *= scale
  up *= scale
  return { t, down, up, tunnelDown: down * 0.68, tunnelUp: up * 0.71 }
}

const RANGE_STEPS: Record<UsageRange, [count: number, step: number]> = {
  fiveMinutes: [300, 1],
  threeHours: [180, 60],
  day: [1440, 60],
  week: [168, 3600],
  month: [720, 3600],
}

export function timeline(range: UsageRange): UsageSample[] {
  const [count, step] = RANGE_STEPS[range]
  const end = Math.floor(now() / step) * step
  return Array.from({ length: count }, (_, i) => sample(end - (count - 1 - i) * step, step))
}

/** The whole span, even the part of today still ahead - a full day reads better. */
export function spanTimeline(fromTs: number, toTs: number): SpanTimeline {
  const bucketSecs = toTs - fromTs <= DAY * 1.1 ? 60 : 3600
  const samples: UsageSample[] = []
  for (let t = Math.floor(fromTs / bucketSecs) * bucketSecs; t <= toTs; t += bucketSecs) {
    samples.push(sample(t, bucketSecs))
  }
  return { bucketSecs, samples }
}

export const span = (): UsageSpan => ({
  firstTs: now() - 74 * DAY,
  lastTs: now(),
  retentionDays: 90,
})

type Share = [key: string, share: number, country?: string]

/** Splits a total across named rows, largest first, the rest folded into `more`. */
function ranked(rows: Share[], down: number, up: number, more: number): Ranked {
  const entries = rows.map(([key, share, country], index) => {
    const d = down * share
    const u = up * share * (index % 3 === 1 ? 1.6 : 0.8)
    return { key, down: d, up: u, total: d + u, ...(country ? { country } : {}) }
  })
  const rest = Math.max(0, 1 - rows.reduce((sum, [, share]) => sum + share, 0))
  return {
    entries,
    more: more > 0 ? { count: more, down: down * rest, up: up * rest } : null,
  }
}

const APPS: Share[] = [
  ['chrome.exe', 0.31],
  ['steam.exe', 0.19],
  ['Spotify.exe', 0.09],
  ['Code.exe', 0.07],
  ['Discord.exe', 0.06],
  ['msedge.exe', 0.05],
  ['OneDrive.exe', 0.04],
  ['Teams.exe', 0.03],
  ['svchost.exe', 0.02],
]

const HOSTS: Share[] = [
  ['googlevideo.com', 0.22, 'US'],
  ['steamcontent.com', 0.17, 'SG'],
  ['spotifycdn.com', 0.08, 'SE'],
  ['github.com', 0.06, 'US'],
  ['discord.media', 0.05, 'DE'],
  ['akamaized.net', 0.05, 'NL'],
  ['microsoft.com', 0.04, 'IE'],
  ['cloudflare.com', 0.03, 'US'],
  ['fastly.net', 0.02, 'JP'],
]

const KINDS: Share[] = [
  ['HTTPS', 0.57],
  ['QUIC', 0.25],
  ['Streaming', 0.07],
  ['Real-time', 0.05],
  ['File transfer', 0.03],
  ['DNS', 0.01],
  ['Other', 0.02],
]

const columns = (down: number, up: number): UsageColumns => ({
  apps: ranked(APPS, down, up, 31),
  hosts: ranked(HOSTS, down, up, 214),
  kinds: ranked(KINDS, down, up, 0),
})

export function summary(fromTs: number, toTs: number): UsageSummary {
  const days = Math.max(1, Math.round((toTs - fromTs) / DAY) + 1)
  const totalDown = 3_920_000_000 * days * (days > 1 ? 0.83 : 1)
  const totalUp = totalDown * 0.118
  const tunnelDown = totalDown * 0.56
  const tunnelUp = totalUp * 0.48
  return {
    fromTs,
    toTs,
    totalUp,
    totalDown,
    tunnelUp,
    tunnelDown,
    openwire: columns(tunnelDown, tunnelUp),
    device: columns(totalDown, totalUp),
    monitoredFromTs: fromTs,
    attributionSupported: true,
  }
}

export const deviceApps = (): Ranked => columns(3_920_000_000, 460_000_000).apps

// ---------------------------------------------------------------------------
// Local network
// ---------------------------------------------------------------------------

const device = (
  ip: number,
  name: string | null,
  kind: Device['kind'],
  vendor: string | null,
  extra: Partial<Device> = {},
): Device => ({
  mac: `02:00:5e:10:00:${ip.toString(16).padStart(2, '0')}`,
  ip: `192.168.1.${ip}`,
  name,
  hostname: name,
  nameSource: name ? 'mdns' : null,
  customName: null,
  vendor,
  privateAddress: false,
  kind,
  kindIsCustom: false,
  role: null,
  online: true,
  firstSeen: now() - 40 * DAY,
  lastSeen: now() - 60,
  isNew: false,
  ...extra,
})

export const devices = (): DevicesSnapshot => ({
  supported: true,
  network: {
    id: 'home',
    adapter: 'Wi-Fi',
    wireless: true,
    subnet: '192.168.1.0/24',
    address: '192.168.1.20',
    gateway: '192.168.1.1',
  },
  devices: [
    device(1, 'Router', 'router', 'TP-Link', { role: 'router', nameSource: null }),
    device(20, 'Workstation', 'computer', 'Intel', { role: 'thisDevice', nameSource: 'netbios' }),
    device(23, 'Pixel 8', 'phone', 'Google'),
    device(27, 'iPhone', 'phone', null, { privateAddress: true }),
    device(31, 'iPad', 'tablet', 'Apple'),
    device(40, 'Living Room TV', 'tv', 'Samsung'),
    device(44, 'Kitchen Speaker', 'speaker', 'Sonos'),
    device(52, 'PlayStation 5', 'console', 'Sony', { online: false, lastSeen: now() - 3 * 3600 }),
    device(60, 'Office Printer', 'printer', 'HP'),
    device(71, 'Hue Bridge', 'smartHome', 'Signify'),
    device(80, 'NAS', 'storage', 'Synology'),
    device(97, null, 'unknown', 'Espressif', { isNew: true, firstSeen: now() - 300 }),
  ],
  scanning: false,
  lastScan: now() - 120,
})
