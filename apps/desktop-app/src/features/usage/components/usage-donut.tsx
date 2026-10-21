/** Totals ring: incoming vs outgoing for the chosen scope, with the split between Maizy and direct traffic below it. */

import { Cell, Pie, PieChart, ResponsiveContainer } from 'recharts'
import { motion } from 'motion/react'

import { formatBytes } from '@/lib/format'
import { staggerChild } from '@/lib/motion'
import type { UsageSummary } from '@/lib/types'

export type UsageScope = 'all' | 'maizy'

export function UsageDonut({ summary, scope }: { summary: UsageSummary; scope: UsageScope }) {
  const all = scope === 'all'
  const down = all ? summary.totalDown : summary.tunnelDown
  const up = all ? summary.totalUp : summary.tunnelUp
  const total = down + up

  const deviceTotal = summary.totalDown + summary.totalUp
  const tunnel = summary.tunnelDown + summary.tunnelUp
  // Clamped: attributed bytes are counted at the proxy and system bytes at the
  // adapter, so rounding could otherwise push this fractionally negative.
  const direct = Math.max(0, deviceTotal - tunnel)
  const share = deviceTotal > 0 ? Math.min(100, (tunnel / deviceTotal) * 100) : 0

  const slices = [
    { name: 'Incoming', value: down, color: 'var(--chart-down)' },
    { name: 'Outgoing', value: up, color: 'var(--chart-up)' },
  ].filter((slice) => slice.value > 0)

  return (
    // Sized for a narrow leading column, so the rankings keep the width.
    <motion.div variants={staggerChild} className="flex flex-col items-center gap-4 px-4 py-5">
      <div className="relative size-36">
        <ResponsiveContainer width="100%" height="100%">
          <PieChart>
            <Pie
              data={slices.length ? slices : [{ name: 'Idle', value: 1, color: 'var(--muted)' }]}
              dataKey="value"
              innerRadius={52}
              outerRadius={70}
              startAngle={90}
              endAngle={-270}
              paddingAngle={slices.length > 1 ? 2 : 0}
              cornerRadius={slices.length > 1 ? 4 : 0}
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
          <p className="text-[10px] tracking-wide text-muted-foreground uppercase">
            {all ? 'Total' : 'Through Maizy'}
          </p>
          <p className="numeric text-lg font-semibold">{formatBytes(total)}</p>
        </div>
      </div>

      <dl className="grid w-full grid-cols-2 gap-x-3 gap-y-3">
        <Figure label="Incoming" value={formatBytes(down)} dot="var(--chart-down)" />
        <Figure label="Outgoing" value={formatBytes(up)} dot="var(--chart-up)" />
        {all ? (
          <>
            <Figure label="Through Maizy" value={formatBytes(tunnel)} dot="var(--primary)" />
            <Figure label="Direct" value={formatBytes(direct)} dot="var(--muted-foreground)" />
          </>
        ) : (
          <>
            <Figure label="Share of all" value={`${share.toFixed(share < 10 ? 1 : 0)}%`} dot="var(--primary)" />
            <Figure label="Direct" value={formatBytes(direct)} dot="var(--muted-foreground)" />
          </>
        )}
      </dl>
    </motion.div>
  )
}

function Figure({ label, value, dot }: { label: string; value: string; dot: string }) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
        <span aria-hidden className="size-1.5 rounded-full" style={{ backgroundColor: dot }} />
        {label}
      </dt>
      <dd className="numeric text-sm font-semibold">{value}</dd>
    </div>
  )
}
