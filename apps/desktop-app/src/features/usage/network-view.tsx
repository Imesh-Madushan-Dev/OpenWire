/** Network: the live throughput graph. */

import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { PauseIcon, PlayIcon } from '@hugeicons/core-free-icons'

import { Segmented } from '@/components/segmented'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { formatSpeed } from '@/lib/format'
import type { UsageRange } from '@/lib/types'

import { ChartPaletteSelect } from './components/chart-palette-select'
import { ThroughputChart } from './components/throughput-chart'
import { BUCKET_SECONDS, SCALE_OPTIONS } from './tiers'
import { useTimeline } from './use-usage'

const RANGES = [
  { value: 'fiveMinutes' as const, label: '5 min' },
  { value: 'threeHours' as const, label: '3 hours' },
  { value: 'day' as const, label: '24 hours' },
  { value: 'week' as const, label: 'Week' },
  { value: 'month' as const, label: 'Month' },
]

export function NetworkView() {
  const [range, setRange] = useState<UsageRange>('fiveMinutes')
  const [scale, setScale] = useState(0)
  const [paused, setPaused] = useState(false)

  const points = useTimeline(range, paused)
  const latest = points.at(-1)
  const live = range === 'fiveMinutes'

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <header className="flex shrink-0 items-center justify-between gap-4 px-8 pt-1 pb-4">
        <div>
          <h1 className="font-heading text-xl font-semibold">Network</h1>
          <p className="text-xs text-muted-foreground">
            Throughput across this device, sampled every second.
          </p>
        </div>
        <Segmented
          options={RANGES}
          value={range}
          onChange={setRange}
          layoutId="network-range"
          size="sm"
          aria-label="Time range"
        />
      </header>

      <div className="flex min-h-0 flex-1 flex-col gap-3 px-8 pb-6">
        <div className="flex shrink-0 items-center gap-2">
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
              className="grid size-8 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-tint/8 hover:text-foreground focus-visible:ring-2 focus-visible:ring-primary/50 focus-visible:outline-none"
            >
              <HugeiconsIcon
                icon={paused ? PlayIcon : PauseIcon}
                size={14}
                strokeWidth={2}
              />
            </button>
          )}

          <ChartPaletteSelect className="ml-auto" />
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
