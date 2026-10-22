/**
 * GlassWire's direction markers: a disc in the series' chart colour with an
 * arrow cut into it. Used wherever incoming and outgoing are told apart - the
 * graph legend, the live readout, the usage totals - so a direction reads the
 * same everywhere.
 */

import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowDown02Icon, ArrowUp02Icon } from '@hugeicons/core-free-icons'
import { cn } from 'cn'

export type Direction = 'down' | 'up'

export function SeriesIcon({
  direction,
  size = 14,
  className,
}: {
  direction: Direction
  size?: number
  className?: string
}) {
  return (
    <span
      aria-hidden
      className={cn(
        'inline-grid shrink-0 place-items-center rounded-full text-background',
        direction === 'down' ? 'bg-chart-down' : 'bg-chart-up',
        className,
      )}
      style={{ width: size, height: size }}
    >
      <HugeiconsIcon
        icon={direction === 'down' ? ArrowDown02Icon : ArrowUp02Icon}
        size={Math.round(size * 0.72)}
        strokeWidth={3}
      />
    </span>
  )
}
