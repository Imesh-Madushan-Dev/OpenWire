/** App shell: window chrome with the main tabs, then the active view. */

import { useState } from 'react'
import { AnimatePresence, motion } from 'motion/react'
import { cn } from 'cn'

import type { ViewId } from '@/components/nav-tabs'
import { NoticeBar } from '@/components/notice-bar'
import { TitleBar } from '@/components/title-bar'
import { viewVariants } from '@/lib/motion'
import { useMaximized } from '@/lib/use-maximized'
import { useDevices } from '@/features/devices/devices-context'
import { DevicesProvider } from '@/features/devices/devices-provider'
import { DevicesView } from '@/features/devices/devices-view'
import type { SectionId } from '@/features/settings/sections'
import { SettingsView } from '@/features/settings/settings-view'
import { NetworkView } from '@/features/usage/network-view'
import { UsageView } from '@/features/usage/usage-view'
import { useVpn } from '@/features/vpn/vpn-context'
import { VpnProvider } from '@/features/vpn/vpn-provider'
import { VpnView } from '@/features/vpn/vpn-view'

export default function App() {
  return (
    <VpnProvider>
      <DevicesProvider>
        <Shell />
      </DevicesProvider>
    </VpnProvider>
  )
}

function Shell() {
  const { connection, notice, notify, dismissNotice, toggle, ready } = useVpn()
  const [view, setView] = useState<ViewId>('vpn')
  // Where Settings opens when something links into it; a new request
  // remounts the view on that section.
  const [settingsEntry, setSettingsEntry] = useState<{ section: SectionId; seq: number }>({
    section: 'protection',
    seq: 0,
  })
  const maximized = useMaximized()
  const { newCount } = useDevices()

  const openSettings = (section: SectionId = 'protection') => {
    setSettingsEntry((current) => ({ section, seq: current.seq + 1 }))
    setView('settings')
  }

  // Arriving on Settings from its tab starts on Protection again, rather than
  // wherever the last deep link left it.
  const navigate = (next: ViewId) => {
    if (next === 'settings' && view !== 'settings') openSettings()
    else setView(next)
  }

  return (
    // The window is transparent and shadowless (tauri.conf.json), so these
    // corners and this border are the window's real edge, matching the 8px
    // Windows 11 gives its own windows. Maximised, the window meets the
    // screen edges, so it squares off.
    <div
      className={cn(
        'shell-canvas relative flex h-full flex-col overflow-hidden bg-background',
        !maximized && 'rounded-[8px] border border-tint/12',
      )}
    >
      <TitleBar
        view={view}
        onNavigate={navigate}
        onOpenSettings={openSettings}
        badges={{ devices: newCount }}
        phase={connection.phase}
        ready={ready}
        onToggleVpn={() => void toggle()}
        onFeedback={(message) => notify({ level: 'info', message })}
      />

      <main className="relative min-h-0 flex-1">
        <AnimatePresence mode="wait" initial={false}>
          <motion.div
            key={view}
            variants={viewVariants}
            initial="initial"
            animate="animate"
            exit="exit"
            className="absolute inset-0"
          >
            {view === 'vpn' && <VpnView onOpenSettings={openSettings} />}
            {view === 'network' && <NetworkView />}
            {view === 'usage' && <UsageView />}
            {view === 'devices' && <DevicesView />}
            {view === 'settings' && (
              <SettingsView key={settingsEntry.seq} initialSection={settingsEntry.section} />
            )}
          </motion.div>
        </AnimatePresence>
      </main>

      <NoticeBar notice={notice} onDismiss={dismissNotice} />
    </div>
  )
}
