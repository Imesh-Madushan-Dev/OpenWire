/** The server selector row beneath the orb. */

import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowDown01Icon, GlobalIcon } from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'

import { formatLatency, latencyQuality } from '@/lib/format'
import { staggerChild } from '@/lib/motion'
import type { ServerSummary, Settings } from '@/lib/types'

import { RegionChip } from './region-chip'

interface LocationCardProps {
  server: ServerSummary | null
  settings: Settings | null
  onOpen: () => void
}

const DOT_TONE = {
  good: 'bg-primary',
  fair: 'bg-[color:var(--warning)]',
  poor: 'bg-destructive',
  unknown: 'bg-muted-foreground/50',
} as const

export function LocationCard({ server, settings, onOpen }: LocationCardProps) {
  // With no explicit preference the backend picks, so say so rather than
  // naming a server the user never chose.
  const automatic = !settings?.preferredServer
  const title = automatic ? 'Optimal location' : (server?.name ?? 'Choose a location')

  const subtitle = server
    ? automatic
      ? `Picking the fastest server - currently ${server.name}`
      : [server.city || server.country, formatLatency(server.latencyMs)]
          .filter(Boolean)
          .join(' · ')
    : 'No server set up yet'

  return (
    <motion.button
      type="button"
      variants={staggerChild}
      onClick={onOpen}
      whileTap={{ scale: 0.99 }}
      className="panel-interactive flex w-full items-center gap-3 rounded-2xl px-4 py-3.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-primary/50"
    >
      <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-white/5">
        {automatic || !server ? (
          <HugeiconsIcon
            icon={GlobalIcon}
            size={18}
            strokeWidth={1.8}
            className="text-primary"
          />
        ) : (
          <RegionChip countryCode={server.countryCode} />
        )}
      </span>

      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm font-semibold">{title}</span>
        <span className="block truncate text-xs text-muted-foreground">{subtitle}</span>
      </span>

      {server?.latencyMs != null && (
        <span
          aria-hidden
          className={`size-1.5 rounded-full ${DOT_TONE[latencyQuality(server.latencyMs)]}`}
        />
      )}

      <HugeiconsIcon
        icon={ArrowDown01Icon}
        size={18}
        strokeWidth={2}
        className="shrink-0 text-muted-foreground"
      />
    </motion.button>
  )
}
