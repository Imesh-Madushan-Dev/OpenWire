/**
 * The VPN page, kept to one decision: connect or not, and where to.
 *
 * The toolbar holds the two protections worth flipping from here. The left
 * pane is one status line, one button, and a hint naming exactly what the
 * button will do; the session's figures sit in a band along its foot. The
 * server list stays in view on the right, so changing location is one click.
 */

import { HugeiconsIcon } from '@hugeicons/react'
import { Copy01Icon } from '@hugeicons/core-free-icons'
import { AnimatePresence, motion } from 'motion/react'
import { cn } from 'cn'

import { Toolbar, ToolbarButton } from '@/components/toolbar'
import { swapVariants } from '@/lib/motion'
import { isActive, isBusy, isRouting } from '@/lib/types'
import { ServerPanel } from '@/features/servers/server-panel'

import { PowerOrb } from './components/power-orb'
import { QuickToggles } from './components/quick-toggles'
import { StatsStrip } from './components/stats-strip'
import { StatusHeadline } from './components/status-headline'
import { useVpn } from './vpn-context'

export function VpnView({ onOpenSettings }: { onOpenSettings: () => void }) {
  const { connection, stats, targetServer, settings, toggle, ready, notify } = useVpn()

  const active = isActive(connection.phase)
  const exposed = connection.phase === 'disconnected' || connection.phase === 'monitoring'
  const automatic = settings?.preferredServer == null

  // What clicking will do, named - so the button never connects somewhere
  // the user did not expect.
  const hint = isBusy(connection.phase)
    ? 'One moment…'
    : active
      ? 'Click to disconnect'
      : targetServer
        ? `Click to connect to ${automatic ? `the fastest server (${targetServer.name})` : targetServer.name}`
        : 'Add a server on the right to get started'

  const copyProxy = (address: string) => {
    void navigator.clipboard
      .writeText(address)
      .then(() => notify({ level: 'info', message: 'Copied the proxy address.' }))
      .catch(() => notify({ level: 'error', message: "Couldn't copy the proxy address." }))
  }

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <Toolbar
        start={<QuickToggles />}
        end={
          <>
            {/* Apps that ignore the system proxy need this address by hand. */}
            {isRouting(connection.phase) && connection.listenAddr && (
              <ToolbarButton
                onClick={() => copyProxy(connection.listenAddr!)}
                title="Copy the local proxy address"
                className="text-muted-foreground"
              >
                <span>Proxy</span>
                <span className="numeric font-mono text-[11px] text-foreground/90">{connection.listenAddr}</span>
                <HugeiconsIcon icon={Copy01Icon} size={12} strokeWidth={2} />
              </ToolbarButton>
            )}
            <ToolbarButton onClick={() => onOpenSettings()}>More settings</ToolbarButton>
          </>
        }
      />

      <div className="flex min-h-0 flex-1">
        <section aria-label="Connection" className="relative flex min-w-0 flex-1 flex-col overflow-hidden">
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

          <div className="relative flex min-h-0 flex-1 flex-col items-center justify-center gap-5 px-6">
            <StatusHeadline connection={connection} />

            <PowerOrb phase={connection.phase} onToggle={() => void toggle()} disabled={!ready} />

            <div className="grid h-5 w-full place-items-center">
              <AnimatePresence mode="wait" initial={false}>
                <motion.p
                  key={hint}
                  variants={swapVariants}
                  initial="initial"
                  animate="animate"
                  exit="exit"
                  className="max-w-sm truncate text-center text-xs text-muted-foreground"
                >
                  {hint}
                </motion.p>
              </AnimatePresence>
            </div>

            {/* A note only appears when there is something the user must know. */}
            <AnimatePresence>
              {connection.note && (
                <motion.p
                  key={connection.note}
                  initial={{ opacity: 0, y: 6 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: 6 }}
                  className="absolute inset-x-6 bottom-3 text-center text-xs text-warning"
                  role="status"
                >
                  {connection.note}
                </motion.p>
              )}
            </AnimatePresence>
          </div>

          <StatsStrip stats={stats} live={active} />
        </section>

        <ServerPanel />
      </div>
    </div>
  )
}
