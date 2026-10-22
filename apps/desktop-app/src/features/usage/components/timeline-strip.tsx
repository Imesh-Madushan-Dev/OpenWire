/**
 * GlassWire's overview timeline: the whole period in miniature on a recessed
 * strip, the part the page is showing lit up, and a scrubber underneath to
 * move or resize that part.
 *
 * The strip always spans its full `domain`, so hours that have not happened
 * yet stay visibly empty rather than the drawing stretching to fill them.
 */

import { useId, useMemo } from 'react'
import { Area, AreaChart, ResponsiveContainer, XAxis, YAxis } from 'recharts'
import { cn } from 'cn'

import { RangeSlider } from '@/components/range-slider'
import { formatTick, tickStep, timeTicks } from '@/lib/time-ticks'

export interface StripPoint {
  t: number
  down: number
  up: number
}

export interface StripSelection {
  /** Positions run from 0 to `steps`, spread evenly across the domain. */
  steps: number
  value: readonly [number, number]
  onChange: (value: [number, number]) => void
  /** Once a drag ends, for callers that refetch on a new bracket. */
  onCommit?: (value: [number, number]) => void
  /** The narrowest bracket allowed, in steps. */
  minSpan?: number
  /** How far Page Up/Down moves a knob, in steps. */
  largeStep?: number
  label: string
  formatValue?: (position: number, thumb: number) => string
}

interface TimelineStripProps {
  points: StripPoint[]
  domain: readonly [number, number]
  /** Omitted when there is nothing to choose, e.g. a single day. */
  selection?: StripSelection
  /** Shown over the strip when there are no points to draw. */
  emptyLabel?: string
  className?: string
}

export function TimelineStrip({ points, domain, selection, emptyLabel, className }: TimelineStripProps) {
  const id = useId().replace(/[^a-zA-Z0-9_-]/g, '')
  const [from, to] = domain
  const span = Math.max(1, to - from)

  const ticks = useMemo(() => timeTicks(from, to, 7), [from, to])
  const step = tickStep(span, 7)

  const lit = selection
    ? {
        left: (selection.value[0] / selection.steps) * 100,
        width: ((selection.value[1] - selection.value[0]) / selection.steps) * 100,
      }
    : null
  const whole = lit !== null && lit.left <= 0 && lit.width >= 100

  return (
    <div className={cn('shrink-0 border-t border-hairline bg-well px-3 pt-2 pb-1.5', className)}>
      {/* Inset by half a knob: the scrubber shares the strip's width, so each
          knob's centre sits exactly on an edge of the lit part. */}
      <div className="px-2">
        <div className="relative h-10 overflow-hidden rounded-[2px]">
          {lit && !whole && (
            <div
              aria-hidden
              className="absolute inset-y-0 border-x border-tint/25 bg-tint/7"
              style={{ left: `${lit.left}%`, width: `${lit.width}%` }}
            />
          )}

          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={points} margin={{ top: 2, right: 0, bottom: 0, left: 0 }}>
              <defs>
                <linearGradient id={`${id}-down`} x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor="var(--chart-down)" stopOpacity={0.85} />
                  <stop offset="100%" stopColor="var(--chart-down)" stopOpacity={0.45} />
                </linearGradient>
              </defs>
              <XAxis dataKey="t" type="number" domain={[from, to]} hide allowDataOverflow />
              <YAxis hide domain={[0, 'auto']} />
              <Area
                type="monotone"
                dataKey="down"
                stroke="var(--chart-down)"
                strokeWidth={1}
                fill={`url(#${id}-down)`}
                isAnimationActive={false}
                dot={false}
                activeDot={false}
              />
              <Area
                type="monotone"
                dataKey="up"
                stroke="var(--chart-up)"
                strokeWidth={1}
                fill="var(--chart-up)"
                fillOpacity={0.5}
                isAnimationActive={false}
                dot={false}
                activeDot={false}
              />
            </AreaChart>
          </ResponsiveContainer>

          {points.length === 0 && emptyLabel && (
            <p className="absolute inset-0 grid place-items-center text-[11px] text-muted-foreground">
              {emptyLabel}
            </p>
          )}
        </div>

        {/* Labels placed by time, not spread evenly, so they line up with the strip. */}
        <div aria-hidden className="relative h-4 overflow-hidden">
          {ticks.map((tick) => {
            const at = ((tick - from) / span) * 100
            // A label hanging off either end would be clipped mid-word.
            if (at < 3 || at > 97) return null
            return (
              <span
                key={tick}
                className="numeric absolute top-0.5 -translate-x-1/2 text-[10px] whitespace-nowrap text-muted-foreground"
                style={{ left: `${at}%` }}
              >
                {formatTick(tick, step)}
              </span>
            )
          })}
        </div>

        {selection && (
          <RangeSlider
            steps={selection.steps}
            value={selection.value}
            onValueChange={selection.onChange}
            onValueCommit={selection.onCommit}
            minSpan={selection.minSpan}
            largeStep={selection.largeStep}
            aria-label={selection.label}
            formatValue={selection.formatValue}
            className="mt-0.5"
          />
        )}
      </div>
    </div>
  )
}
