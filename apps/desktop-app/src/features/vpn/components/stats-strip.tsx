/**
 * The session readout along the bottom of the connect pane, in GlassWire's
 * banded style: four cells divided by grooves. Always present, so the pane
 * never reflows on connect; the figures read as dashes while the tunnel is
 * down.
 */

import { HugeiconsIcon } from '@hugeicons/react'
import { FlashIcon, Time01Icon } from '@hugeicons/core-free-icons'
import { cn } from 'cn'

import { SeriesIcon } from '@/components/series-icon'
import { formatDuration, formatLatency, formatSpeed, latencyQuality } from '@/lib/format'
import type { StatsSnapshot } from '@/lib/types'

const QUALITY_TONE = {
  good: 'text-success',
  fair: 'text-warning',
  poor: 'text-destructive',
  unknown: 'text-muted-foreground',
} as const

export function StatsStrip({ stats, live }: { stats: StatsSnapshot; live: boolean }) {
  const quality = latencyQuality(stats.latencyMs)

  return (
    <dl className="grid h-12 shrink-0 grid-cols-4 divide-x divide-hairline border-t border-hairline bg-rail/60">
      {/* Arrows in the chart colours, so a direction reads the same here as
          on the Network and Usage graphs. */}
      <Cell label="Download" icon={<SeriesIcon direction="down" size={12} />}>
        {live ? formatSpeed(stats.downBps) : '—'}
      </Cell>
      <Cell label="Upload" icon={<SeriesIcon direction="up" size={12} />}>
        {live ? formatSpeed(stats.upBps) : '—'}
      </Cell>
      <Cell
        label="Latency"
        icon={<HugeiconsIcon icon={FlashIcon} size={12} strokeWidth={2.2} className={QUALITY_TONE[quality]} />}
        className={live ? QUALITY_TONE[quality] : undefined}
      >
        {live ? formatLatency(stats.latencyMs) : '—'}
      </Cell>
      <Cell
        label="Session"
        icon={<HugeiconsIcon icon={Time01Icon} size={12} strokeWidth={2.2} className="text-muted-foreground" />}
      >
        {live ? formatDuration(stats.uptimeSecs) : '—'}
      </Cell>
    </dl>
  )
}

function Cell({
  label,
  icon,
  className,
  children,
}: {
  label: string
  icon: React.ReactNode
  className?: string
  children: React.ReactNode
}) {
  return (
    <div className="flex min-w-0 flex-col justify-center gap-0.5 px-4">
      <dt className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
        {icon}
        {label}
      </dt>
      <dd className={cn('numeric truncate text-[13px] font-semibold', className)}>{children}</dd>
    </div>
  )
}
