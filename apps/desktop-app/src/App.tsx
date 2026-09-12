/** App shell: window chrome, navigation rail, and the active view. */

import { useState } from 'react'
import { AnimatePresence, motion } from 'motion/react'

import { NavRail, type ViewId } from '@/components/nav-rail'
import { NoticeBar } from '@/components/notice-bar'
import { TitleBar } from '@/components/title-bar'
import { viewVariants } from '@/lib/motion'
import { isActive } from '@/lib/types'
import { LicenseGate } from '@/features/license/license-gate'
import { SettingsView } from '@/features/settings/settings-view'
import { NetworkView } from '@/features/usage/network-view'
import { UsageView } from '@/features/usage/usage-view'
import { useVpn } from '@/features/vpn/vpn-context'
import { VpnProvider } from '@/features/vpn/vpn-provider'
import { VpnView } from '@/features/vpn/vpn-view'

export default function App() {
  return (
    // Nothing VPN-related mounts until the license is active: the gate owns the
    // window until then, and the backend refuses to start a tunnel regardless.
    <LicenseGate>
      <VpnProvider>
        <Shell />
      </VpnProvider>
    </LicenseGate>
  )
}

function Shell() {
  const { connection, notice, notify, dismissNotice } = useVpn()
  const [view, setView] = useState<ViewId>('vpn')

  return (
    // The window is frameless, so the app paints its own rounded shell.
    <div className="flex h-full overflow-hidden rounded-xl border border-white/8 bg-background">
      <NavRail active={view} onChange={setView} connected={isActive(connection.phase)} />

      <div className="relative flex min-w-0 flex-1 flex-col">
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
              {view === 'vpn' && <VpnView onOpenFeatures={() => setView('settings')} />}
              {view === 'network' && <NetworkView />}
              {view === 'usage' && <UsageView />}
              {view === 'settings' && <SettingsView />}
            </motion.div>
          </AnimatePresence>
        </main>

        <NoticeBar notice={notice} onDismiss={dismissNotice} />
      </div>
    </div>
  )
}
