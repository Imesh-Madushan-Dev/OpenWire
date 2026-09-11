/** The dashboard: status, the connect control, and the two setup cards. */

import { useState } from 'react'
import { AnimatePresence, motion } from 'motion/react'

import { staggerParent, swapVariants } from '@/lib/motion'
import { isActive, isBusy } from '@/lib/types'
import { ServerSheet } from '@/features/servers/server-sheet'

import { ActionCard } from './components/action-card'
import { LocationCard } from './components/location-card'
import { PowerOrb } from './components/power-orb'
import { StatsStrip } from './components/stats-strip'
import { StatusHeadline } from './components/status-headline'
import { useVpn } from './vpn-context'

export function VpnView({ onOpenFeatures }: { onOpenFeatures: () => void }) {
  const { connection, stats, settings, targetServer, toggle, ready } = useVpn()
  const [pickerOpen, setPickerOpen] = useState(false)

  const active = isActive(connection.phase)

  return (
    <div className="relative flex h-full flex-col overflow-hidden">
      <div aria-hidden className="ambient-field pointer-events-none absolute inset-0" />

      <div className="relative flex flex-1 flex-col items-center justify-center gap-9 px-8">
        <StatusHeadline connection={connection} />

        <PowerOrb phase={connection.phase} onToggle={() => void toggle()} disabled={!ready} />

        {/* Reserved slot: the hint and the readout swap without moving the orb. */}
        <div className="grid h-[76px] w-full place-items-center">
          <AnimatePresence mode="wait" initial={false}>
            {active ? (
              <motion.div
                key="stats"
                variants={swapVariants}
                initial="initial"
                animate="animate"
                exit="exit"
              >
                <StatsStrip stats={stats} />
              </motion.div>
            ) : (
              <motion.p
                key="hint"
                variants={swapVariants}
                initial="initial"
                animate="animate"
                exit="exit"
                className="text-sm text-muted-foreground"
              >
                {isBusy(connection.phase) ? 'One moment' : 'Tap to connect'}
              </motion.p>
            )}
          </AnimatePresence>
        </div>
      </div>

      {/* A note only appears when there is something the user must know. */}
      <AnimatePresence>
        {connection.note && (
          <motion.p
            key={connection.note}
            initial={{ opacity: 0, y: 6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 6 }}
            className="relative mx-auto mb-3 max-w-md px-8 text-center text-xs text-[color:var(--warning)]"
            role="status"
          >
            {connection.note}
          </motion.p>
        )}
      </AnimatePresence>

      <motion.div
        variants={staggerParent}
        initial="initial"
        animate="animate"
        className="relative mx-auto flex w-full max-w-md flex-col gap-2.5 px-8 pb-8"
      >
        <LocationCard
          server={targetServer}
          settings={settings}
          onOpen={() => setPickerOpen(true)}
        />
        <ActionCard
          onAddServer={() => setPickerOpen(true)}
          onOpenFeatures={onOpenFeatures}
        />
      </motion.div>

      <ServerSheet open={pickerOpen} onOpenChange={setPickerOpen} />
    </div>
  )
}
