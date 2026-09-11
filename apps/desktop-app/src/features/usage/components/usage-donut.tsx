/** Totals ring: incoming vs outgoing, with the tunnel share called out. */

import { Cell, Pie, PieChart, ResponsiveContainer } from 'recharts'
import { motion } from 'motion/react'

import { formatBytes } from '@/lib/format'
import { staggerChild } from '@/lib/motion'
import type { UsageSummary } from '@/lib/types'

export function UsageDonut({ summary }: { summary: UsageSummary }) {
  const total = summary.totalDown + summary.totalUp
  const tunnel = summary.tunnelDown + summary.tunnelUp
  // Clamped: attributed bytes are counted at the proxy and system bytes at the
  // adapter, so rounding could otherwise push this fractionally negative.
  const direct = Math.max(0, total - tunnel)

  const slices = [
    { name: 'Incoming', value: summary.totalDown, color: 'var(--chart-down)' },
    { name: 'Outgoing', value: summary.totalUp, color: 'var(--chart-up)' },
  ].filter((slice) => slice.value > 0)

  return (
    <motion.div
      variants={staggerChild}
      className="panel flex flex-col items-center gap-5 rounded-2xl px-5 py-6"
    >
      <div className="relative size-[176px]">
        <ResponsiveContainer width="100%" height="100%">
          <PieChart>
            <Pie
              data={slices.length ? slices : [{ name: 'Idle', value: 1, color: 'var(--muted)' }]}
              dataKey="value"
              innerRadius={62}
              outerRadius={84}
              startAngle={90}
              endAngle={-270}
              paddingAngle={slices.length > 1 ? 2 : 0}
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
          <p className="text-[10px] tracking-wide text-muted-foreground uppercase">Total</p>
          <p className="numeric text-xl font-semibold">{formatBytes(total)}</p>
        </div>
      </div>

      <dl className="grid w-full grid-cols-2 gap-x-4 gap-y-3">
        <Figure label="Incoming" value={summary.totalDown} dot="var(--chart-down)" />
        <Figure label="Outgoing" value={summary.totalUp} dot="var(--chart-up)" />
        <Figure label="Through Maizy" value={tunnel} dot="var(--primary)" />
        <Figure label="Direct" value={direct} dot="var(--muted-foreground)" />
      </dl>
    </motion.div>
  )
}

function Figure({ label, value, dot }: { label: string; value: number; dot: string }) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
        <span aria-hidden className="size-1.5 rounded-full" style={{ backgroundColor: dot }} />
        {label}
      </dt>
      <dd className="numeric text-sm font-semibold">{formatBytes(value)}</dd>
    </div>
  )
}
