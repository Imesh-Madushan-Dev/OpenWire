/**
 * Live session readout. Replaces the "Tap to connect" hint once a tunnel is up,
 * occupying the same slot so the layout never jumps.
 */

import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowDown01Icon,
  ArrowUp01Icon,
  FlashIcon,
  Time01Icon,
} from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'
import { cn } from 'cn'

import { formatDuration, formatLatency, formatSpeed, latencyQuality } from '@/lib/format'
import { staggerChild, staggerParent } from '@/lib/motion'
import type { StatsSnapshot } from '@/lib/types'

const QUALITY_TONE = {
  good: 'text-success',
  fair: 'text-warning',
  poor: 'text-destructive',
  unknown: 'text-muted-foreground',
} as const

export function StatsStrip({ stats }: { stats: StatsSnapshot }) {
  const quality = latencyQuality(stats.latencyMs)

  return (
    <motion.dl
      variants={staggerParent}
      initial="initial"
      animate="animate"
      className="flex items-stretch gap-2"
    >
      <Stat
        icon={ArrowDown01Icon}
        label="Download"
        value={formatSpeed(stats.downBps)}
        tone="text-foreground"
      />
      <Stat
        icon={ArrowUp01Icon}
        label="Upload"
        value={formatSpeed(stats.upBps)}
        tone="text-foreground"
      />
      <Stat
        icon={FlashIcon}
        label="Latency"
        value={formatLatency(stats.latencyMs)}
        tone={QUALITY_TONE[quality]}
      />
      <Stat
        icon={Time01Icon}
        label="Session"
        value={formatDuration(stats.uptimeSecs)}
        tone="text-foreground"
      />
    </motion.dl>
  )
}

interface StatProps {
  icon: typeof FlashIcon
  label: string
  value: string
  tone: string
}

function Stat({ icon, label, value, tone }: StatProps) {
  return (
    <motion.div
      variants={staggerChild}
      className="panel flex min-w-[104px] flex-col gap-1 rounded-2xl px-4 py-3"
    >
      <dt className="flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
        <HugeiconsIcon icon={icon} size={13} strokeWidth={2} />
        {label}
      </dt>
      <dd className={cn('numeric text-sm font-semibold', tone)}>{value}</dd>
    </motion.div>
  )
}
