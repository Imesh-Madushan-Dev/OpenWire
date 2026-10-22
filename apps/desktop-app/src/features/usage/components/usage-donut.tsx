/**
 * The Usage page's left panel, after GlassWire's: a thin incoming/outgoing
 * ring around the total, the two directions in large type, and a pair of
 * mini bar charts splitting what went through Maizy from what went direct.
 */

import type { ReactNode } from 'react'
import { Cell, Pie, PieChart, ResponsiveContainer } from 'recharts'
import { motion } from 'motion/react'

import { formatBytes } from '@/lib/format'
import { staggerChild } from '@/lib/motion'
import type { UsageSummary } from '@/lib/types'

export type UsageScope = 'all' | 'maizy'

interface UsageDonutProps {
  summary: UsageSummary
  scope: UsageScope
  /** Rendered in the panel's top-right corner, where GlassWire keeps its filter. */
  corner?: ReactNode
  /** The days counted, when they are only part of the chosen period. */
  caption?: string
}

export function UsageDonut({ summary, scope, corner, caption }: UsageDonutProps) {
  const all = scope === 'all'
  const down = all ? summary.totalDown : summary.tunnelDown
  const up = all ? summary.totalUp : summary.tunnelUp
  const total = down + up

  // Clamped: attributed bytes are counted at the proxy and system bytes at the
  // adapter, so rounding could otherwise push these fractionally negative.
  const direct = {
    down: Math.max(0, summary.totalDown - summary.tunnelDown),
    up: Math.max(0, summary.totalUp - summary.tunnelUp),
  }
  const tunnel = { down: summary.tunnelDown, up: summary.tunnelUp }
  const tallest = Math.max(direct.down, direct.up, tunnel.down, tunnel.up)

  const slices = [
    { name: 'Incoming', value: down, color: 'var(--chart-down)' },
    { name: 'Outgoing', value: up, color: 'var(--chart-up)' },
  ].filter((slice) => slice.value > 0)

  return (
    <motion.section
      variants={staggerChild}
      aria-label="Totals"
      className="relative flex min-h-0 flex-col items-center gap-3 overflow-hidden px-4 pt-4 pb-3"
    >
      {caption && (
        <p className="absolute top-2.5 left-3 max-w-[calc(100%-3rem)] truncate text-[11px] font-medium text-primary">
          {caption}
        </p>
      )}
      {corner && <div className="absolute top-2 right-2">{corner}</div>}

      <div className="relative size-28 shrink-0 [@media(min-height:640px)]:size-36">
        <ResponsiveContainer width="100%" height="100%">
          <PieChart>
            <Pie
              data={slices.length ? slices : [{ name: 'Idle', value: 1, color: 'var(--muted)' }]}
              dataKey="value"
              innerRadius="80%"
              outerRadius="100%"
              startAngle={90}
              endAngle={-270}
              stroke="none"
              isAnimationActive={false}
            >
              {(slices.length ? slices : [{ color: 'var(--muted)' }]).map((slice, index) => (
                <Cell key={index} fill={slice.color} />
              ))}
            </Pie>
          </PieChart>
        </ResponsiveContainer>

        {/* Overlaid rather than an SVG label, so it inherits app typography. */}
        <div className="pointer-events-none absolute inset-0 grid place-content-center text-center">
          <p className="text-[11px] text-muted-foreground">{all ? 'Total' : 'Through Maizy'}</p>
          <p className="numeric text-lg leading-tight font-semibold">{formatBytes(total)}</p>
        </div>
      </div>

      <dl className="grid w-full grid-cols-2 gap-x-3">
        <Figure label="Incoming" value={formatBytes(down)} />
        <Figure label="Outgoing" value={formatBytes(up)} />
      </dl>

      {/* Bars on a shared scale, so the pair reads as a comparison. */}
      <div aria-hidden className="grid w-full shrink-0 grid-cols-2 gap-x-3">
        <MiniBars pair={tunnel} tallest={tallest} />
        <MiniBars pair={direct} tallest={tallest} />
      </div>

      <dl className="-mt-2 grid w-full grid-cols-2 gap-x-3">
        <Figure label="Through Maizy" value={formatBytes(tunnel.down + tunnel.up)} small />
        <Figure label="Direct" value={formatBytes(direct.down + direct.up)} small />
      </dl>
    </motion.section>
  )
}

function Figure({ label, value, small = false }: { label: string; value: string; small?: boolean }) {
  return (
    <div className="flex min-w-0 flex-col">
      <dt className="truncate text-[11px] text-muted-foreground">{label}</dt>
      <dd className={small ? 'numeric text-sm font-semibold' : 'numeric text-lg leading-tight font-semibold'}>
        {value}
      </dd>
    </div>
  )
}

/** Incoming and outgoing as two short columns, GlassWire's external/local glyph. */
function MiniBars({ pair, tallest }: { pair: { down: number; up: number }; tallest: number }) {
  const height = (bytes: number) =>
    tallest > 0 && bytes > 0 ? Math.max(2, Math.round((bytes / tallest) * 36)) : 0

  return (
    <span className="flex h-9 items-end gap-0.5 border-b border-tint/10">
      <span className="w-3.5 bg-chart-down" style={{ height: height(pair.down) }} />
      <span className="w-3.5 bg-chart-up" style={{ height: height(pair.up) }} />
    </span>
  )
}
