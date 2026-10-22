/** Headline + exit location, the two lines above the orb. */

import { AnimatePresence, motion } from 'motion/react'
import { cn } from 'cn'

import { swapVariants } from '@/lib/motion'
import type { ConnectionState, Phase } from '@/lib/types'

import { RegionChip } from './region-chip'

// Tunnel words, not protection words: Maizy disguises where traffic goes,
// it does not promise to keep anyone safe.
const HEADLINE: Record<Phase, string> = {
  disconnected: 'Tunnel is off',
  connecting: 'Opening the tunnel',
  connected: 'Tunnel is on',
  reconnecting: 'Tunnel interrupted',
  disconnecting: 'Closing the tunnel',
  monitoring: 'Tunnel is off, measuring usage',
}

const TONE: Record<Phase, string> = {
  disconnected: 'text-exposed',
  connecting: 'text-foreground',
  connected: 'text-protected',
  reconnecting: 'text-warning',
  disconnecting: 'text-muted-foreground',
  // Not tunnelled, so it must not borrow the tunnel-up colour.
  monitoring: 'text-exposed',
}

export function StatusHeadline({ connection }: { connection: ConnectionState }) {
  const { phase, serverName, exitHost, countryCode } = connection
  // Monitor mode has no exit location worth showing - traffic leaves from
  // this device, which is what "Tunnel is off" already says.
  const showLocation =
    phase !== 'disconnected' && phase !== 'monitoring' && Boolean(serverName)

  return (
    <div className="flex flex-col items-center gap-2 text-center">
      {/* Fixed height: the headline swap must not nudge the orb. */}
      <div className="grid h-8 place-items-center">
        <AnimatePresence mode="wait" initial={false}>
          <motion.p
            key={phase}
            variants={swapVariants}
            initial="initial"
            animate="animate"
            exit="exit"
            className={cn('text-glow text-xl font-semibold', TONE[phase])}
          >
            {HEADLINE[phase]}
          </motion.p>
        </AnimatePresence>
      </div>

      <div className="grid h-6 place-items-center">
        <AnimatePresence mode="wait" initial={false}>
          {showLocation ? (
            <motion.div
              key="location"
              variants={swapVariants}
              initial="initial"
              animate="animate"
              exit="exit"
              className="flex items-center gap-2 text-sm"
            >
              <RegionChip countryCode={countryCode} size="sm" />
              <span className="font-medium text-foreground/90">{serverName}</span>
              {exitHost && (
                <>
                  <span aria-hidden className="text-muted-foreground/50">
                    &middot;
                  </span>
                  <span className="numeric text-muted-foreground" data-selectable>
                    {exitHost}
                  </span>
                </>
              )}
            </motion.div>
          ) : (
            <motion.p
              key="idle"
              variants={swapVariants}
              initial="initial"
              animate="animate"
              exit="exit"
              className="text-sm text-muted-foreground"
            >
              {phase === 'monitoring'
                ? 'Traffic goes out directly; Maizy is only counting it'
                : 'Traffic goes out directly, without the disguise'}
            </motion.p>
          )}
        </AnimatePresence>
      </div>
    </div>
  )
}
