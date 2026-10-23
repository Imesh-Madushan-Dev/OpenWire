/**
 * The app with a pretend backend, for README screenshots.
 *
 * Loaded by `screenshots.html` in a plain browser - never by the Tauri build.
 * Tauri's IPC mocks answer every command from `fixtures.ts`, and live events
 * tick once a second, so the real UI renders exactly as it would on a busy
 * machine without touching any real data.
 *
 *   ?view=settings&section=appearance&theme=neon&mode=light
 *   &update=0.4.0 shows that release as available
 */

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { emit } from '@tauri-apps/api/event'
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'

import '../index.css'
import App from '../App.tsx'
import { applyChartPalette, readChartPalette } from '../lib/chart-palette'
import { applyAppearance, readMode, readTheme } from '../lib/theme'
import type { UsageRange } from '../lib/types'
import * as data from './fixtures'

const params = new URLSearchParams(location.search)

// The keys `lib/theme.ts` reads, set before it does.
localStorage.setItem('openwire.colorTheme', params.get('theme') ?? 'neon')
localStorage.setItem('openwire.theme', params.get('mode') ?? 'light')

mockWindows('main')
mockIPC(
  (cmd, args) => {
    const a = (args ?? {}) as Record<string, number & UsageRange>
    switch (cmd) {
      case 'vpn_status':
        return data.connected()
      case 'vpn_logs':
        return data.logs()
      case 'servers_list':
        return data.servers
      case 'servers_optimal':
        return data.servers[0]
      case 'settings_get':
        return data.settings
      case 'platform_info':
        return data.platform
      case 'update_status':
      case 'update_check':
        return data.update(params.get('update'))
      case 'usage_timeline':
        return data.timeline(a.range)
      case 'usage_timeline_between':
        return data.spanTimeline(a.fromTs, a.toTs)
      case 'usage_summary_between':
        return data.summary(a.fromTs, a.toTs)
      case 'usage_span':
        return data.span()
      case 'usage_device_apps':
        return data.deviceApps()
      case 'usage_icons':
        return {}
      case 'netmon_status':
        return data.monitor
      case 'devices_snapshot':
        return data.devices()
      case 'plugin:autostart|is_enabled':
        return true
      case 'plugin:window|is_maximized':
        return false
      default:
        return null
    }
  },
  { shouldMockEvents: true },
)

applyAppearance(readTheme(), readMode())
applyChartPalette(readChartPalette())

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)

let tick = 0
setInterval(() => {
  tick += 1
  const [latest] = data.timeline('fiveMinutes').slice(-1)
  void emit('vpn:stats', data.stats(tick))
  void emit('usage:sample', latest)
}, 1000)

/** Clicks the first button under `scope` whose text includes `label`, once it exists. */
function press(scope: string, label: string, then?: () => void) {
  const button = [...document.querySelectorAll(`${scope} button`)].find((candidate) =>
    // `includes`: a tab's badge count shares its text.
    candidate.textContent?.trim().toLowerCase().includes(label),
  )
  if (button instanceof HTMLElement) {
    button.click()
    if (then) setTimeout(then, 300)
  } else {
    setTimeout(() => press(scope, label, then), 100)
  }
}

// Opens the requested tab, and section, the way a person would.
const view = params.get('view')
const section = params.get('section')
if (view) {
  setTimeout(() => press('header', view, section ? () => press('main', section) : undefined), 300)
}
