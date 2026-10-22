/**
 * Network: GlassWire's Graph tab. Live throughput on a full-width graph, the
 * period's overview underneath with a scrubber to zoom into part of it, and
 * the toolbar choosing whose traffic and how far back.
 */

import { useMemo, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowDown01Icon, PauseIcon, PlayIcon } from '@hugeicons/core-free-icons'

import { SeriesIcon } from '@/components/series-icon'
import { Toolbar, ToolbarButton, ToolbarChoice } from '@/components/toolbar'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { formatBytes, formatSpeed } from '@/lib/format'
import type { UsageRange } from '@/lib/types'

import { ThroughputChart, type RatePoint } from './components/throughput-chart'
import { TimelineStrip } from './components/timeline-strip'
import { BUCKET_SECONDS, niceCeiling, RANGE_SECONDS, SCALE_OPTIONS, withGaps } from './tiers'
import { useTimeline } from './use-usage'

type Scope = 'all' | 'maizy'

const SCOPES = [
  { value: 'all' as const, label: 'All traffic' },
  { value: 'maizy' as const, label: 'Through Maizy' },
]

// Longest first, right-aligned, the way GlassWire orders them.
const RANGES = [
  { value: 'month' as const, label: 'Month' },
  { value: 'week' as const, label: 'Week' },
  { value: 'day' as const, label: '24 Hours' },
  { value: 'threeHours' as const, label: '3 Hours' },
  { value: 'fiveMinutes' as const, label: '5 Minutes' },
]

/** Scrubber resolution: fine enough to feel continuous on any tier. */
const STEPS = 1000
const FULL: readonly [number, number] = [0, STEPS]

export function NetworkView() {
  const [range, setRange] = useState<UsageRange>('fiveMinutes')
  const [scope, setScope] = useState<Scope>('all')
  // 0 is automatic: a round figure above the visible peak.
  const [scale, setScale] = useState(0)
  const [paused, setPaused] = useState(false)
  // The scrubber's bracket, dropped whenever the range changes: a bracket on
  // the month means nothing on the live five minutes.
  const [bracket, setBracket] = useState<readonly [number, number]>(FULL)

  const chooseRange = (next: UsageRange) => {
    setRange(next)
    setBracket(FULL)
  }

  const points = useTimeline(range, paused)
  const live = range === 'fiveMinutes'
  const seconds = BUCKET_SECONDS[range]

  // Exactly the range's span, ending at the newest bucket - the backend hands
  // over its last N buckets, which reach further back than the label says
  // whenever the app was closed for a while. Before any samples, it ends now,
  // so the axes are drawn rather than collapsing.
  const [nowTs] = useState(() => Math.floor(Date.now() / 1000))
  const last = points.at(-1)?.t
  const end = last !== undefined ? last + seconds : nowTs
  const full: readonly [number, number] = [end - RANGE_SECONDS[range], end]

  const rates = useMemo<RatePoint[]>(
    () =>
      withGaps(
        points
          .filter((sample) => sample.t >= end - RANGE_SECONDS[range])
          .map((sample) => ({
            t: sample.t,
            down: (scope === 'all' ? sample.down : sample.tunnelDown) / seconds,
            up: (scope === 'all' ? sample.up : sample.tunnelUp) / seconds,
          })),
        seconds,
      ),
    [points, scope, seconds, end, range],
  )

  const span = full[1] - full[0]
  const viewFrom = full[0] + (span * bracket[0]) / STEPS
  const viewTo = full[0] + (span * bracket[1]) / STEPS

  // A bucket of margin on the left, so a hill cut by the edge still slopes off.
  const visible = useMemo(
    () => rates.filter((point) => point.t >= viewFrom - seconds && point.t <= viewTo),
    [rates, viewFrom, viewTo, seconds],
  )

  const peak = visible.reduce((max, point) => Math.max(max, point.down, point.up), 0)
  const ceiling = scale > 0 ? scale : niceCeiling(peak)

  const latest = rates.at(-1)
  const totals = visible.reduce(
    (sum, point) =>
      point.t >= viewFrom
        ? { down: sum.down + point.down * seconds, up: sum.up + point.up * seconds }
        : sum,
    { down: 0, up: 0 },
  )

  const timeAt = (position: number) =>
    new Date((full[0] + (span * position) / STEPS) * 1000).toLocaleString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit',
      second: live ? '2-digit' : undefined,
    })

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <Toolbar
        start={
          <ToolbarChoice
            options={SCOPES}
            value={scope}
            onChange={setScope}
            layoutId="network-scope"
            aria-label="Traffic shown"
          />
        }
        end={
          <ToolbarChoice
            options={RANGES}
            value={range}
            onChange={chooseRange}
            layoutId="network-range"
            aria-label="Time range"
          />
        }
      />

      <div className="relative min-h-0 flex-1">
        {/* Top-left: the scale, named by its top guide line, as GlassWire does. */}
        <div className="absolute top-1.5 left-2 z-10">
          <ScaleMenu scale={scale} ceiling={ceiling} onChange={setScale} />
        </div>

        {/* Top-right: the live rate, or what moved in the part on screen. */}
        <div className="pointer-events-none absolute top-2 right-3 z-10 flex items-center gap-3">
          {live ? (
            <>
              <Reading direction="down" value={formatSpeed(latest?.down ?? 0)} />
              <Reading direction="up" value={formatSpeed(latest?.up ?? 0)} />
            </>
          ) : (
            <>
              <Reading direction="down" value={formatBytes(totals.down)} />
              <Reading direction="up" value={formatBytes(totals.up)} />
            </>
          )}
          {live && (
            <button
              type="button"
              onClick={() => setPaused((value) => !value)}
              aria-label={paused ? 'Resume live graph' : 'Pause live graph'}
              aria-pressed={paused}
              title={paused ? 'Resume' : 'Pause'}
              className="pointer-events-auto grid size-6 place-items-center rounded-[3px] text-muted-foreground transition-colors hover:bg-tint/8 hover:text-foreground focus-visible:ring-2 focus-visible:ring-primary/50 focus-visible:outline-none"
            >
              <HugeiconsIcon icon={paused ? PlayIcon : PauseIcon} size={13} strokeWidth={2} />
            </button>
          )}
        </div>

        <ThroughputChart points={visible} domain={[viewFrom, viewTo]} ceiling={ceiling} />

        {rates.length === 0 && (
          <p className="pointer-events-none absolute inset-x-0 top-1/2 -translate-y-1/2 text-center text-xs text-muted-foreground">
            {live ? 'Waiting for the first second of traffic…' : 'Nothing recorded in this range yet.'}
          </p>
        )}

        {/* GlassWire's legend, at the left end of the axis band. */}
        <div
          aria-hidden
          className="absolute bottom-0 left-0 z-10 flex h-6 items-center gap-1 bg-rail pr-2 pl-2"
        >
          <SeriesIcon direction="down" />
          <SeriesIcon direction="up" />
        </div>
      </div>

      <TimelineStrip
        points={rates}
        domain={full}
        selection={{
          steps: STEPS,
          value: bracket,
          onChange: setBracket,
          minSpan: 20,
          largeStep: STEPS / 10,
          label: 'Shown period',
          formatValue: timeAt,
        }}
      />
    </div>
  )
}

function Reading({ direction, value }: { direction: 'down' | 'up'; value: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <SeriesIcon direction={direction} size={12} />
      <span className="sr-only">{direction === 'down' ? 'Incoming' : 'Outgoing'}</span>
      <span className="numeric text-[11px] font-semibold">{value}</span>
    </span>
  )
}

function ScaleMenu({
  scale,
  ceiling,
  onChange,
}: {
  scale: number
  ceiling: number
  onChange: (scale: number) => void
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={<ToolbarButton aria-label={`Graph scale: ${formatSpeed(ceiling)}`} />}
      >
        <span className="numeric text-muted-foreground">{formatSpeed(ceiling)}</span>
        <HugeiconsIcon icon={ArrowDown01Icon} size={12} strokeWidth={2} className="text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-44">
        <DropdownMenuLabel>Graph scale</DropdownMenuLabel>
        <DropdownMenuRadioGroup
          value={scale}
          onValueChange={(value: number) => onChange(value)}
        >
          <DropdownMenuRadioItem closeOnClick value={0}>Automatic</DropdownMenuRadioItem>
          <DropdownMenuSeparator />
          {SCALE_OPTIONS.map((option) => (
            <DropdownMenuRadioItem closeOnClick key={option.value} value={option.value}>
              {option.label}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
