/** App shell: window chrome, the active view, and the floating dock. */

import { useState } from 'react'
import { AnimatePresence, motion } from 'motion/react'

import { Dock, type ViewId } from '@/components/dock'
import { NoticeBar } from '@/components/notice-bar'
import { TitleBar } from '@/components/title-bar'
import { viewVariants } from '@/lib/motion'
import { isActive } from '@/lib/types'
import { SettingsView } from '@/features/settings/settings-view'
import { NetworkView } from '@/features/usage/network-view'
import { UsageView } from '@/features/usage/usage-view'
import { useVpn } from '@/features/vpn/vpn-context'
import { VpnProvider } from '@/features/vpn/vpn-provider'
import { VpnView } from '@/features/vpn/vpn-view'

export default function App() {
  return (
    <VpnProvider>
      <Shell />
    </VpnProvider>
  )
}

function Shell() {
  const { connection, notice, notify, dismissNotice } = useVpn()
  const [view, setView] = useState<ViewId>('vpn')

  return (
    // The window is frameless, so the app paints its own rounded shell.
    <div className="relative flex h-full flex-col overflow-hidden rounded-xl border border-tint/8 bg-background">
      <TitleBar onFeedback={(message) => notify({ level: 'info', message })} />

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
            {view === 'settings' && <SettingsView />}
          </motion.div>
        </AnimatePresence>
      </main>

      <Dock active={view} onChange={setView} connected={isActive(connection.phase)} />
      <NoticeBar notice={notice} onDismiss={dismissNotice} />
    </div>
  )
}
