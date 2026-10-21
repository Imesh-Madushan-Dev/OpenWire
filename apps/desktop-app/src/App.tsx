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
  const { connection, notice, notify, dismissNotice } = useVpn()
  const [view, setView] = useState<ViewId>('vpn')
  const maximized = useMaximized()
  const { newCount } = useDevices()

  return (
    // The window is transparent and shadowless (tauri.conf.json), so these
    // corners and this border are the window's real edge - Windows caps its
    // own rounding at 8px. Maximised, the window meets the screen edges, so
    // it squares off.
    <div
      className={cn(
        'relative flex h-full flex-col overflow-hidden bg-background',
        !maximized && 'rounded-[12px] border border-tint/12',
      )}
    >
      <TitleBar
        view={view}
        onNavigate={setView}
        badges={{ devices: newCount }}
        phase={connection.phase}
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
            {view === 'vpn' && <VpnView onOpenSettings={() => setView('settings')} />}
            {view === 'network' && <NetworkView />}
            {view === 'usage' && <UsageView />}
            {view === 'devices' && <DevicesView />}
            {view === 'settings' && <SettingsView />}
          </motion.div>
        </AnimatePresence>
      </main>

      <NoticeBar notice={notice} onDismiss={dismissNotice} />
    </div>
  )
}
