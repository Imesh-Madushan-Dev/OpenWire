/**
 * The main throughput graph, drawn the way GlassWire draws it: solid, smooth
 * incoming and outgoing hills on a flat field, two quiet guide lines at the
 * ceiling and half of it, and the time axis on a raised band underneath.
 *
 * Points arrive already converted to bytes per second by the caller, so the
 * same traffic draws at the same height on every tier.
 */

import { useId, useMemo } from 'react'
import { Area, AreaChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'

import { SeriesIcon } from '@/components/series-icon'
import { formatSpeed } from '@/lib/format'
import { formatTick, tickStep, timeTicks } from '@/lib/time-ticks'

export interface RatePoint {
  t: number
  down: number
  up: number
}

interface ThroughputChartProps {
  points: RatePoint[]
  /** The instants the x axis spans, in Unix seconds. */
  domain: readonly [number, number]
  /** Y-axis ceiling in bytes per second. */
  ceiling: number
}

/** Height of the axis band, shared by the chart and the band painted behind it. */
const AXIS_HEIGHT = 24

export function ThroughputChart({ points, domain, ceiling }: ThroughputChartProps) {
  // Gradient ids must be unique per chart: the overview strip draws its own.
  const id = useId().replace(/[^a-zA-Z0-9_-]/g, '')
  const [from, to] = domain

  const ticks = useMemo(() => timeTicks(from, to, 7), [from, to])
  const step = tickStep(to - from, 7)

  return (
    <div className="relative size-full">
      {/* The raised band the axis labels sit on, under the transparent chart. */}
      <div
        aria-hidden
        className="absolute inset-x-0 bottom-0 border-t border-hairline bg-rail"
        style={{ height: AXIS_HEIGHT }}
      />

      <ResponsiveContainer width="100%" height="100%">
        {/* Room above the ceiling's guide line for the scale label sitting on it. */}
        <AreaChart data={points} margin={{ top: 30, right: 0, bottom: 0, left: 0 }}>
          <defs>
            <linearGradient id={`${id}-down`} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--chart-down)" stopOpacity={0.95} />
              <stop offset="100%" stopColor="var(--chart-down)" stopOpacity={0.55} />
            </linearGradient>
            <linearGradient id={`${id}-up`} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--chart-up)" stopOpacity={0.8} />
              <stop offset="100%" stopColor="var(--chart-up)" stopOpacity={0.45} />
            </linearGradient>
          </defs>

          <CartesianGrid vertical={false} stroke="var(--border)" />

          <XAxis
            dataKey="t"
            type="number"
            domain={[from, to]}
            ticks={ticks}
            tickFormatter={(t: number) => formatTick(t, step)}
            allowDataOverflow
            height={AXIS_HEIGHT}
            tickLine={false}
            axisLine={false}
            interval="preserveStartEnd"
            tick={{ fill: 'var(--muted-foreground)', fontSize: 10 }}
          />
          <YAxis
            hide
            domain={[0, ceiling]}
            ticks={[ceiling / 2, ceiling]}
            // A fixed ceiling must clip rather than rescale, or it is not fixed.
            allowDataOverflow
          />

          <Tooltip
            cursor={{ stroke: 'var(--muted-foreground)', strokeWidth: 1, strokeDasharray: '2 3' }}
            content={<RateTooltip />}
            isAnimationActive={false}
          />

          {/* Incoming behind outgoing: it is almost always the larger hill, and
              the translucent outgoing fill turns orange where they overlap,
              as GlassWire's does. */}
          <Area
            type="monotone"
            dataKey="down"
            stroke="var(--chart-down)"
            strokeWidth={1}
            fill={`url(#${id}-down)`}
            isAnimationActive={false}
            dot={false}
            activeDot={{ r: 3, strokeWidth: 0, fill: 'var(--chart-down)' }}
          />
          <Area
            type="monotone"
            dataKey="up"
            stroke="var(--chart-up)"
            strokeWidth={1}
            fill={`url(#${id}-up)`}
            isAnimationActive={false}
            dot={false}
            activeDot={{ r: 3, strokeWidth: 0, fill: 'var(--chart-up)' }}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  )
}

interface TooltipProps {
  active?: boolean
  payload?: { dataKey?: string | number; value?: number }[]
  label?: number
}

function RateTooltip({ active, payload, label }: TooltipProps) {
  if (!active || !payload?.length) return null

  const read = (key: string) => payload.find((entry) => entry.dataKey === key)?.value ?? 0

  return (
    <div className="flex flex-col gap-1 rounded-md border border-tint/10 bg-popover/95 px-2.5 py-2 shadow-xl backdrop-blur">
      <p className="numeric text-[10px] text-muted-foreground">
        {typeof label === 'number'
          ? new Date(label * 1000).toLocaleString(undefined, {
              month: 'short',
              day: 'numeric',
              hour: 'numeric',
              minute: '2-digit',
              second: '2-digit',
            })
          : ''}
      </p>
      <TooltipRow direction="down" label="Incoming" value={read('down')} />
      <TooltipRow direction="up" label="Outgoing" value={read('up')} />
    </div>
  )
}

function TooltipRow({
  direction,
  label,
  value,
}: {
  direction: 'down' | 'up'
  label: string
  value: number
}) {
  return (
    <div className="flex items-center gap-2 text-xs">
      <SeriesIcon direction={direction} size={12} />
      <span className="text-muted-foreground">{label}</span>
      <span className="numeric ml-auto pl-3 font-medium">{formatSpeed(value)}</span>
    </div>
  )
}
