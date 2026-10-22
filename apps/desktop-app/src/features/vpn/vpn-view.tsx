/**
 * The dashboard, in two panes: status and the connect control on the left,
 * the server list on the right - always in view, nothing to open first.
 */

import { AnimatePresence, motion } from 'motion/react'
import { cn } from 'cn'

import { swapVariants } from '@/lib/motion'
import { isActive, isBusy } from '@/lib/types'
import { ServerPanel } from '@/features/servers/server-panel'

import { PowerOrb } from './components/power-orb'
import { QuickToggles } from './components/quick-toggles'
import { StatsStrip } from './components/stats-strip'
import { StatusHeadline } from './components/status-headline'
import { useVpn } from './vpn-context'

export function VpnView({ onOpenSettings }: { onOpenSettings: () => void }) {
  const { connection, stats, targetServer, settings, toggle, ready } = useVpn()

  const active = isActive(connection.phase)
  const exposed = connection.phase === 'disconnected' || connection.phase === 'monitoring'
  const automatic = settings?.preferredServer == null

  // What tapping will do, named - so the orb never connects somewhere the
  // user did not expect.
  const hint = isBusy(connection.phase)
    ? 'One moment'
    : targetServer
      ? `Tap to connect to ${automatic ? 'the fastest server' : targetServer.name}`
      : 'Add a server to get started'

  return (
    <div className="flex h-full">
      <section className="relative flex min-w-0 flex-1 flex-col overflow-hidden">
        {/* Two washes cross-fading: covered while connected, exposed while
            off. Neither shows mid-transition, when the state is in doubt. */}
        <div
          aria-hidden
          className={cn(
            'ambient-field pointer-events-none absolute inset-0 transition-opacity duration-700',
            connection.phase === 'connected' ? 'opacity-100' : 'opacity-0',
          )}
        />
        <div
          aria-hidden
          className={cn(
            'ambient-exposed pointer-events-none absolute inset-0 transition-opacity duration-700',
            exposed ? 'opacity-100' : 'opacity-0',
          )}
        />

        <div className="relative flex flex-1 flex-col items-center justify-center gap-8 px-8">
          <StatusHeadline connection={connection} />

          <PowerOrb phase={connection.phase} onToggle={() => void toggle()} disabled={!ready} />

          {/* Reserved slot: the hint and the readout swap without moving the orb. */}
          <div className="grid h-19 w-full place-items-center">
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
                  key={hint}
                  variants={swapVariants}
                  initial="initial"
                  animate="animate"
                  exit="exit"
                  className="max-w-xs truncate text-center text-sm text-muted-foreground"
                >
                  {hint}
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
              className="relative mx-auto mb-3 max-w-md px-8 text-center text-xs text-warning"
              role="status"
            >
              {connection.note}
            </motion.p>
          )}
        </AnimatePresence>

        <div className="relative px-8 pb-7">
          <QuickToggles onOpenSettings={onOpenSettings} />
        </div>
      </section>

      <ServerPanel />
    </div>
  )
}
