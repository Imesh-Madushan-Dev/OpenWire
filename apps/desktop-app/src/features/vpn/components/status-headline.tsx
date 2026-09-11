/** Headline + exit location, the two lines above the orb. */

import { AnimatePresence, motion } from 'motion/react'
import { cn } from 'cn'

import { swapVariants } from '@/lib/motion'
import type { ConnectionState, Phase } from '@/lib/types'

import { RegionChip } from './region-chip'

const HEADLINE: Record<Phase, string> = {
  disconnected: 'VPN is off',
  connecting: 'Securing your connection',
  connected: 'You are protected',
  reconnecting: 'Connection interrupted',
  disconnecting: 'Closing the tunnel',
}

const TONE: Record<Phase, string> = {
  disconnected: 'text-muted-foreground',
  connecting: 'text-foreground',
  connected: 'text-foreground',
  reconnecting: 'text-warning',
  disconnecting: 'text-muted-foreground',
}

export function StatusHeadline({ connection }: { connection: ConnectionState }) {
  const { phase, serverName, exitHost, countryCode } = connection
  const showLocation = phase !== 'disconnected' && Boolean(serverName)

  return (
    <div className="flex flex-col items-center gap-2 text-center">
      {/* Fixed height: the headline swap must not nudge the orb. */}
      <div className="grid h-7 place-items-center">
        <AnimatePresence mode="wait" initial={false}>
          <motion.p
            key={phase}
            variants={swapVariants}
            initial="initial"
            animate="animate"
            exit="exit"
            className={cn('text-[15px] font-medium', TONE[phase])}
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
              Your traffic is exposed to your network
            </motion.p>
          )}
        </AnimatePresence>
      </div>
    </div>
  )
}
