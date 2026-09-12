/** Network usage: a live throughput graph and a ranked breakdown. */

import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ComputerIcon,
  GlobalIcon,
  PauseIcon,
  PlayIcon,
  Route01Icon,
} from '@hugeicons/core-free-icons'
import { AnimatePresence, motion } from 'motion/react'

import { Segmented } from '@/components/segmented'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { formatSpeed } from '@/lib/format'
import { staggerParent, swapVariants } from '@/lib/motion'
import type { UsageRange } from '@/lib/types'

import { BreakdownColumn } from './components/breakdown-column'
import { ThroughputChart } from './components/throughput-chart'
import { UsageDonut } from './components/usage-donut'
import { BUCKET_SECONDS, SCALE_OPTIONS } from './tiers'
import { useSummary, useTimeline } from './use-usage'

type Tab = 'graph' | 'usage'

const TABS = [
  { value: 'graph' as const, label: 'Graph' },
  { value: 'usage' as const, label: 'Usage' },
]

const GRAPH_RANGES = [
  { value: 'fiveMinutes' as const, label: '5 min' },
  { value: 'threeHours' as const, label: '3 hours' },
  { value: 'day' as const, label: '24 hours' },
  { value: 'week' as const, label: 'Week' },
  { value: 'month' as const, label: 'Month' },
]

const USAGE_RANGES = [
  { value: 'day' as const, label: 'Today' },
  { value: 'week' as const, label: '7 days' },
  { value: 'month' as const, label: '30 days' },
]

export function UsageView() {
  const [tab, setTab] = useState<Tab>('graph')

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <header className="flex shrink-0 items-center justify-between gap-4 px-8 pt-1 pb-4">
        <div>
          <h1 className="font-heading text-xl font-semibold">Network</h1>
          <p className="text-xs text-muted-foreground">
            Live throughput and where your bandwidth went.
          </p>
        </div>
        <Segmented
          options={TABS}
          value={tab}
          onChange={setTab}
          layoutId="usage-tab"
          aria-label="Usage view"
        />
      </header>

      <div className="relative min-h-0 flex-1">
        <AnimatePresence mode="wait" initial={false}>
          <motion.div
            key={tab}
            variants={swapVariants}
            initial="initial"
            animate="animate"
            exit="exit"
            className="absolute inset-0"
          >
            {tab === 'graph' ? <GraphTab /> : <UsageTab />}
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  )
}

function GraphTab() {
  const [range, setRange] = useState<UsageRange>('fiveMinutes')
  const [scale, setScale] = useState(0)
  const [paused, setPaused] = useState(false)

  const points = useTimeline(range, paused)
  const latest = points.at(-1)
  const live = range === 'fiveMinutes'

  return (
    <div className="flex h-full flex-col gap-3 px-8 pb-6">
      <div className="flex shrink-0 flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          {/* `items` lets the trigger show the label rather than the raw
              byte-rate value the graph needs. */}
          <Select
            items={SCALE_OPTIONS}
            value={scale}
            onValueChange={(value) => setScale(value ?? 0)}
          >
            <SelectTrigger size="sm" aria-label="Graph scale" className="text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {SCALE_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          {/* Pausing only makes sense where points are still arriving. */}
          {live && (
            <button
              type="button"
              onClick={() => setPaused((value) => !value)}
              aria-label={paused ? 'Resume' : 'Pause'}
              aria-pressed={paused}
              className="grid size-8 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-white/8 hover:text-foreground focus-visible:ring-2 focus-visible:ring-primary/50 focus-visible:outline-none"
            >
              <HugeiconsIcon
                icon={paused ? PlayIcon : PauseIcon}
                size={14}
                strokeWidth={2}
              />
            </button>
          )}
        </div>

        <Segmented
          options={GRAPH_RANGES}
          value={range}
          onChange={setRange}
          layoutId="usage-range"
          size="sm"
          aria-label="Time range"
        />
      </div>

      <div className="panel relative min-h-0 flex-1 rounded-2xl p-3">
        {/* Current rate, read straight off the newest bucket. */}
        <div className="pointer-events-none absolute top-4 left-4 z-10 flex gap-4">
          <Reading
            color="var(--chart-down)"
            label="Down"
            value={latest ? latest.down / BUCKET_SECONDS[range] : 0}
          />
          <Reading
            color="var(--chart-up)"
            label="Up"
            value={latest ? latest.up / BUCKET_SECONDS[range] : 0}
          />
          {paused && (
            <span className="self-center rounded-full bg-warning/15 px-2 py-0.5 text-[10px] font-semibold text-warning">
              Paused
            </span>
          )}
        </div>

        <ThroughputChart points={points} range={range} scale={scale} />
      </div>
    </div>
  )
}

function Reading({ color, label, value }: { color: string; label: string; value: number }) {
  return (
    <div className="flex items-center gap-1.5">
      <span aria-hidden className="size-1.5 rounded-full" style={{ backgroundColor: color }} />
      <span className="text-[10px] text-muted-foreground">{label}</span>
      <span className="numeric text-[11px] font-semibold">{formatSpeed(value)}</span>
    </div>
  )
}

function UsageTab() {
  const [range, setRange] = useState<UsageRange>('day')
  const summary = useSummary(range)

  // An empty column has two very different causes, and "Recorded while the VPN
  // is connected" reads as a fault rather than as the reason when the real
  // answer is simply that nothing was tunnelled yet.
  const attributionHint = !summary.attributionSupported
    ? 'Per-app names need Windows.'
    : summary.tunnelUp + summary.tunnelDown === 0
      ? 'No traffic went through Maizy in this period. Connect, or turn on "Measure usage while disconnected" in Features.'
      : 'Recorded while the VPN is connected.'

  return (
    <div className="h-full overflow-y-auto px-8 pb-8">
      <motion.div
        variants={staggerParent}
        initial="initial"
        animate="animate"
        className="flex flex-col gap-4"
      >
        <div className="flex shrink-0 justify-end">
          <Segmented
            options={USAGE_RANGES}
            value={range}
            onChange={setRange}
            layoutId="summary-range"
            size="sm"
            aria-label="Usage period"
          />
        </div>

        <div className="grid grid-cols-1 gap-4 lg:grid-cols-[260px_1fr]">
          <UsageDonut summary={summary} />

          <div className="grid min-w-0 grid-cols-1 gap-4 sm:grid-cols-3">
            <BreakdownColumn
              title="Apps"
              icon={ComputerIcon}
              entries={summary.apps}
              emptyHint={attributionHint}
            />
            <BreakdownColumn
              title="Hosts"
              icon={GlobalIcon}
              entries={summary.hosts}
              emptyHint={attributionHint}
            />
            <BreakdownColumn
              title="Traffic type"
              icon={Route01Icon}
              entries={summary.kinds}
              emptyHint={attributionHint}
            />
          </div>
        </div>

        <p className="px-1 text-xs leading-relaxed text-muted-foreground">
          The ring covers all network activity on this device. The three columns cover only
          traffic that went through Maizy, which is the only traffic it can attribute without
          a kernel driver.
        </p>
      </motion.div>
    </div>
  )
}
