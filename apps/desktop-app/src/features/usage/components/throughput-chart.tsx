/**
 * Live throughput graph.
 *
 * Samples arrive as bytes-per-bucket, so everything is converted to a rate
 * before it is drawn - otherwise the same traffic would appear sixty times
 * larger on the per-minute tiers than on the live one.
 */

import { useMemo } from 'react'
import {
  Area,
  AreaChart,
  Brush,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'

import { formatSpeed } from '@/lib/format'
import type { UsageRange, UsageSample } from '@/lib/types'

import { BUCKET_SECONDS } from '../tiers'

interface ThroughputChartProps {
  points: UsageSample[]
  range: UsageRange
  /** Y-axis ceiling in bytes/sec; 0 means auto. */
  scale: number
}

export function ThroughputChart({ points, range, scale }: ThroughputChartProps) {
  const seconds = BUCKET_SECONDS[range]

  const data = useMemo(
    () =>
      points.map((sample) => ({
        t: sample.t,
        down: sample.down / seconds,
        up: sample.up / seconds,
      })),
    [points, seconds],
  )

  const formatTime = useMemo(() => timeFormatter(range), [range])

  return (
    <ResponsiveContainer width="100%" height="100%">
      <AreaChart data={data} margin={{ top: 8, right: 4, bottom: 0, left: 4 }}>
        <defs>
          <linearGradient id="fill-down" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="var(--chart-down)" stopOpacity={0.45} />
            <stop offset="100%" stopColor="var(--chart-down)" stopOpacity={0.02} />
          </linearGradient>
          <linearGradient id="fill-up" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="var(--chart-up)" stopOpacity={0.4} />
            <stop offset="100%" stopColor="var(--chart-up)" stopOpacity={0.02} />
          </linearGradient>
        </defs>

        <CartesianGrid
          vertical={false}
          stroke="var(--hairline)"
          strokeDasharray="2 6"
        />

        <XAxis
          dataKey="t"
          tickFormatter={formatTime}
          tickLine={false}
          axisLine={false}
          minTickGap={56}
          tick={{ fill: 'var(--muted-foreground)', fontSize: 10 }}
        />
        <YAxis
          width={0}
          tick={false}
          axisLine={false}
          tickLine={false}
          domain={[0, scale > 0 ? scale : 'auto']}
          // A fixed ceiling must clip rather than rescale, or it is not fixed.
          allowDataOverflow={scale > 0}
        />

        <Tooltip
          cursor={{ stroke: 'var(--hairline)', strokeWidth: 1 }}
          content={<UsageTooltip formatTime={formatTime} />}
        />

        {/* Download sits behind upload: it is almost always the larger series. */}
        <Area
          type="monotone"
          dataKey="down"
          stroke="var(--chart-down)"
          strokeWidth={1.5}
          fill="url(#fill-down)"
          isAnimationActive={false}
          dot={false}
          activeDot={{ r: 3, strokeWidth: 0, fill: 'var(--chart-down)' }}
        />
        <Area
          type="monotone"
          dataKey="up"
          stroke="var(--chart-up)"
          strokeWidth={1.5}
          fill="url(#fill-up)"
          isAnimationActive={false}
          dot={false}
          activeDot={{ r: 3, strokeWidth: 0, fill: 'var(--chart-up)' }}
        />

        {/* The scrubber is pointless on the live range, where the window is
            constantly sliding out from under the selection. */}
        {range !== 'fiveMinutes' && data.length > 2 && (
          <Brush
            dataKey="t"
            height={28}
            travellerWidth={8}
            stroke="var(--hairline)"
            fill="color-mix(in oklab, var(--tint) 2%, transparent)"
            tickFormatter={formatTime}
          />
        )}
      </AreaChart>
    </ResponsiveContainer>
  )
}

interface TooltipProps {
  active?: boolean
  payload?: { dataKey?: string | number; value?: number }[]
  label?: number
  formatTime: (t: number) => string
}

function UsageTooltip({ active, payload, label, formatTime }: TooltipProps) {
  if (!active || !payload?.length) return null

  const read = (key: string) =>
    payload.find((entry) => entry.dataKey === key)?.value ?? 0

  return (
    <div className="rounded-xl border border-tint/10 bg-card/95 px-3 py-2 shadow-xl backdrop-blur">
      <p className="numeric mb-1.5 text-[10px] text-muted-foreground">
        {typeof label === 'number' ? formatTime(label) : ''}
      </p>
      <Row color="var(--chart-down)" label="Down" value={read('down')} />
      <Row color="var(--chart-up)" label="Up" value={read('up')} />
    </div>
  )
}

function Row({ color, label, value }: { color: string; label: string; value: number }) {
  return (
    <div className="flex items-center gap-2 text-xs">
      <span
        aria-hidden
        className="size-1.5 rounded-full"
        style={{ backgroundColor: color }}
      />
      <span className="text-muted-foreground">{label}</span>
      <span className="numeric ml-auto font-medium">{formatSpeed(value)}</span>
    </div>
  )
}

/** Axis labels get only as much precision as the tier actually resolves. */
function timeFormatter(range: UsageRange) {
  return (unixSeconds: number) => {
    const date = new Date(unixSeconds * 1000)
    if (range === 'fiveMinutes') {
      return date.toLocaleTimeString(undefined, {
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
      })
    }
    if (range === 'threeHours' || range === 'day') {
      return date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
    }
    return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
  }
}
