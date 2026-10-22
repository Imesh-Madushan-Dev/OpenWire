/**
 * Usage: GlassWire's Usage tab. Totals for a day, week, month or custom run of
 * days, the ranked apps, hosts and traffic types behind them, and the
 * period's traffic along the bottom - for the whole device or only what went
 * through OpenWire.
 */

import { useMemo, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowDown01Icon, InformationCircleIcon, Route01Icon } from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'
import { cn } from 'cn'

import { Toolbar, ToolbarButton, ToolbarChoice } from '@/components/toolbar'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { staggerParent } from '@/lib/motion'

import { BreakdownColumn } from './components/breakdown-column'
import { MonitorPrompt } from './components/monitor-prompt'
import { PeriodPicker } from './components/period-picker'
import { TimelineStrip } from './components/timeline-strip'
import { UsageDonut, type UsageScope } from './components/usage-donut'
import {
  customPeriod,
  firstDayOfWeek,
  periodBounds,
  periodDays,
  periodFor,
  startOfDay,
  subPeriod,
  type Period,
  type PeriodMode,
} from './periods'
import { withGaps } from './tiers'
import { useMonitor } from './use-monitor'
import { useAppIcons, usePeriodTimeline, useSummary, useUsageSpan, useWindowsApps } from './use-usage'

const SCOPES = [
  { value: 'all' as const, label: 'All traffic' },
  { value: 'openwire' as const, label: 'Through OpenWire' },
]

const MODE_LABELS: Record<PeriodMode, string> = {
  day: 'Day',
  week: 'Week',
  month: 'Month',
  custom: 'Custom',
}

/** Remembered per viewer: someone who checks only their VPN traffic should
 *  not have to switch every visit. Storage can be blocked; the default holds. */
const SCOPE_KEY = 'openwire.usage.scope'

function readScope(): UsageScope {
  try {
    return localStorage.getItem(SCOPE_KEY) === 'openwire' ? 'openwire' : 'all'
  } catch {
    return 'all'
  }
}

function saveScope(scope: UsageScope) {
  try {
    localStorage.setItem(SCOPE_KEY, scope)
  } catch {
    // Not remembered; the switch still works for this visit.
  }
}

export function UsageView() {
  const span = useUsageSpan()
  // Read once per mount: the pickers only need it for their bounds.
  // ponytail: a session left open past midnight keeps yesterday as "today".
  // The backend clamps anyway; revisit if anyone actually trips over it.
  const [today] = useState(() => startOfDay(new Date()))
  const [weekStartsOn] = useState(() => firstDayOfWeek())

  // Opens on today, the way GlassWire does.
  const [mode, setMode] = useState<PeriodMode>('day')
  const [anchor, setAnchor] = useState<Date>(today)
  // Defaults to the last week, so opening Custom shows real figures at once.
  const [custom, setCustom] = useState<{ from: Date; to: Date }>(() => ({
    from: new Date(today.getFullYear(), today.getMonth(), today.getDate() - 6),
    to: today,
  }))

  const period = useMemo<Period>(
    () =>
      mode === 'custom'
        ? customPeriod(custom.from, custom.to, today)
        : periodFor(mode, anchor, weekStartsOn, today),
    [mode, anchor, custom, today, weekStartsOn],
  )

  // The scrubber narrows a multi-day period to some of its days. `null` is
  // all of them, and every way of choosing another period resets it.
  const days = useMemo(() => periodDays(period), [period])
  const [bracket, setBracket] = useState<{ draft: Bracket; committed: Bracket } | null>(null)
  const whole: Bracket = [0, days.length]
  const current = bracket ?? { draft: whole, committed: whole }

  const chooseMode = (next: PeriodMode) => {
    setMode(next)
    setBracket(null)
  }
  const chooseAnchor = (next: Date) => {
    setAnchor(next)
    setBracket(null)
  }
  const chooseCustom = (from: Date, to: Date) => {
    setCustom({ from, to })
    setBracket(null)
  }

  const shown = useMemo<Period>(() => {
    const [from, to] = current.committed
    return from === 0 && to === days.length ? period : subPeriod(days, from, to - 1, today)
  }, [current.committed, days, period, today])

  const summary = useSummary(shown)
  const timeline = usePeriodTimeline(period)
  const monitor = useMonitor()

  const [scope, setScopeState] = useState<UsageScope>(readScope)
  const setScope = (next: UsageScope) => {
    setScopeState(next)
    saveScope(next)
  }

  // Which figures fill the columns. The device view exists only for periods
  // the network monitor recorded; without it, Apps falls back to Windows' own
  // per-app history and the other two columns explain what is missing.
  const device = summary.device
  const columns = scope === 'openwire' ? summary.openwire : device
  const fallback = scope === 'all' && !device

  // Seconds to answer, so only asked for when it is actually what is shown.
  const windows = useWindowsApps(summary.fromTs, summary.toTs, fallback)
  const apps = columns?.apps ?? windows.ranked
  const iconFor = useAppIcons(apps.entries.map((entry) => entry.key))

  // An empty OpenWire column has two very different causes, and "nothing was
  // recorded" reads as a fault when the real answer is that nothing went
  // through OpenWire in this period.
  const emptyHint =
    scope === 'all'
      ? 'Nothing was recorded in this period.'
      : !summary.attributionSupported
        ? 'Per-app names need Windows.'
        : summary.tunnelUp + summary.tunnelDown === 0
          ? 'No traffic went through OpenWire in this period. Connect, or turn on "Measure usage while disconnected" in Settings.'
          : 'Recorded while the VPN is connected.'

  const strip = useMemo(
    () =>
      withGaps(
        timeline.samples.map((sample) => ({
          t: sample.t,
          down: scope === 'all' ? sample.down : sample.tunnelDown,
          up: scope === 'all' ? sample.up : sample.tunnelUp,
        })),
        timeline.bucketSecs,
      ),
    [timeline, scope],
  )

  const earliest = span?.firstTs ? startOfDay(new Date(span.firstTs * 1000)) : null

  const dayLabel = (position: number) => {
    const index = Math.min(days.length - 1, Math.max(0, position))
    return new Date(days[index] * 1000).toLocaleDateString(undefined, {
      weekday: 'short',
      month: 'short',
      day: 'numeric',
    })
  }

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <Toolbar
        start={
          <ToolbarChoice
            options={SCOPES}
            value={scope}
            onChange={setScope}
            layoutId="usage-scope"
            aria-label="Traffic shown"
          />
        }
        center={
          <PeriodPicker
            mode={mode}
            period={period}
            anchor={anchor}
            onAnchorChange={chooseAnchor}
            onCustomChange={chooseCustom}
            earliest={earliest}
            today={today}
            weekStartsOn={weekStartsOn}
          />
        }
        end={
          <>
            {scope === 'all' && <MonitorBadge monitor={monitor} />}
            <ModeMenu mode={mode} onChange={chooseMode} />
          </>
        }
      />

      {/* Keyed by scope so a switch replays the entrance and every column
          starts folded again, instead of morphing one list into another. */}
      <motion.div
        key={scope}
        variants={staggerParent}
        initial="initial"
        animate="animate"
        className="grid min-h-0 flex-1 grid-cols-[12.5rem_repeat(3,minmax(0,1fr))] divide-x divide-hairline"
      >
        <UsageDonut
          summary={summary}
          scope={scope}
          // Says so when the scrubber has narrowed the period: the toolbar
          // still names the whole of it.
          caption={shown === period ? undefined : shown.label}
          corner={
            <AboutFigures
              scope={scope}
              fromTs={summary.fromTs}
              monitoredFromTs={summary.monitoredFromTs}
              hasDevice={Boolean(device)}
              retentionDays={span?.retentionDays ?? 90}
            />
          }
        />

        <BreakdownColumn
          title="Apps"
          ranked={apps}
          loading={fallback && windows.loading}
          note={fallback ? "From Windows' records" : undefined}
          emptyHint={emptyHint}
          iconFor={iconFor}
        />

        {columns ? (
          <>
            <BreakdownColumn title="Hosts" ranked={columns.hosts} emptyHint={emptyHint} flags />
            <BreakdownColumn
              title="Traffic Type"
              ranked={columns.kinds}
              emptyHint={emptyHint}
              rowIcon={Route01Icon}
            />
          </>
        ) : (
          <MonitorPrompt
            state={monitor.state}
            error={monitor.status?.error ?? null}
            busy={monitor.busy}
            onEnable={() => void monitor.enable()}
            className="col-span-2"
          />
        )}
      </motion.div>

      <TimelineStrip
        points={strip}
        domain={periodBounds(period)}
        emptyLabel="No timeline kept for this period"
        // One day has no days to choose between.
        selection={
          days.length > 1
            ? {
                steps: days.length,
                value: current.draft,
                onChange: (value) => setBracket({ draft: value, committed: current.committed }),
                onCommit: (value) => setBracket({ draft: value, committed: value }),
                label: 'Days counted',
                // The end knob sits on the boundary after the last day it counts.
                formatValue: (position, thumb) => dayLabel(thumb === 0 ? position : position - 1),
              }
            : undefined
        }
      />
    </div>
  )
}

/** Start and end positions on the period's day boundaries: `[0, days]` is all of it. */
type Bracket = readonly [number, number]

/** GlassWire's "Day ▾": the presets, then Custom below a rule. */
function ModeMenu({ mode, onChange }: { mode: PeriodMode; onChange: (mode: PeriodMode) => void }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={<ToolbarButton aria-label={`Period: ${MODE_LABELS[mode]}`} />}>
        {MODE_LABELS[mode]}
        <HugeiconsIcon icon={ArrowDown01Icon} size={13} strokeWidth={2} className="text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-36 min-w-36">
        <DropdownMenuRadioGroup value={mode} onValueChange={(value: PeriodMode) => onChange(value)}>
          <DropdownMenuRadioItem closeOnClick value="day">Day</DropdownMenuRadioItem>
          <DropdownMenuRadioItem closeOnClick value="week">Week</DropdownMenuRadioItem>
          <DropdownMenuRadioItem closeOnClick value="month">Month</DropdownMenuRadioItem>
          <DropdownMenuSeparator />
          <DropdownMenuRadioItem closeOnClick value="custom">Custom</DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

/** Where the figures come from, behind the panel's corner button. */
function AboutFigures({
  scope,
  fromTs,
  monitoredFromTs,
  hasDevice,
  retentionDays,
}: {
  scope: UsageScope
  fromTs: number
  monitoredFromTs: number | null
  hasDevice: boolean
  retentionDays: number
}) {
  return (
    <Popover>
      <PopoverTrigger
        render={
          <button
            type="button"
            aria-label="Where these figures come from"
            className="grid size-6 place-items-center rounded-[3px] border border-tint/10 text-muted-foreground outline-none transition-colors hover:bg-tint/8 hover:text-foreground focus-visible:ring-2 focus-visible:ring-primary/50 data-popup-open:bg-tint/10"
          />
        }
      >
        <HugeiconsIcon icon={InformationCircleIcon} size={13} strokeWidth={2} />
      </PopoverTrigger>
      <PopoverContent align="start" side="right" className="w-72 gap-2 p-3 text-xs leading-relaxed">
        <p>{footnote(scope, fromTs, monitoredFromTs, hasDevice)}</p>
        <p className="text-muted-foreground">History is kept for {retentionDays} days.</p>
      </PopoverContent>
    </Popover>
  )
}

function footnote(
  scope: UsageScope,
  fromTs: number,
  monitoredFromTs: number | null,
  hasDevice: boolean,
): string {
  if (scope === 'openwire') {
    return 'Only traffic that went through OpenWire: over the VPN, or routed for measuring while disconnected.'
  }
  if (!hasDevice) {
    return "The ring covers all network activity on this device, and Apps comes from Windows' own per-app records. Hosts and traffic types need detailed monitoring."
  }
  // The period began before monitoring did, so its first days are missing.
  const partial = monitoredFromTs !== null && monitoredFromTs > fromTs
  const since = partial
    ? ` since ${new Date(monitoredFromTs * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short' })}`
    : ''
  return `Every app, host and traffic type on this device${since}, recorded by detailed monitoring. Traffic OpenWire carried is credited to the app that sent it.`
}

/**
 * A quiet signal that the device view is live, or that it needs a look - with
 * the fix right beside it. The columns stay filled while an outdated monitor
 * keeps recording, so this is the only place on the page that says so.
 */
function MonitorBadge({ monitor }: { monitor: ReturnType<typeof useMonitor> }) {
  const { state, busy } = monitor
  const troubled = state === 'failing' || state === 'stopped' || state === 'outdated'
  const tone =
    state === 'active'
      ? { label: 'Monitoring', dot: 'bg-success', text: 'text-foreground/80' }
      : state === 'starting'
        ? { label: 'Starting', dot: 'bg-success/50', text: 'text-muted-foreground' }
        : state === 'outdated'
          ? { label: 'Monitor needs an update', dot: 'bg-warning', text: 'text-warning' }
          : troubled
            ? { label: 'Monitor needs attention', dot: 'bg-warning', text: 'text-warning' }
            : null
  if (!tone) return null

  return (
    <span className={cn('mr-1 flex min-w-0 items-center gap-1.5 text-[11px] font-medium', tone.text)}>
      <span className="relative flex size-1.5 shrink-0">
        {state === 'active' && (
          <span aria-hidden className="absolute inset-0 animate-ping rounded-full bg-success opacity-60" />
        )}
        <span className={cn('relative size-1.5 rounded-full', tone.dot)} />
      </span>
      <span className="truncate">{tone.label}</span>
      {troubled && (
        <Button
          variant="outline"
          size="xs"
          className="ml-1 h-5 px-2 text-[11px]"
          disabled={busy}
          onClick={() => void monitor.enable()}
        >
          {busy ? 'Waiting…' : state === 'outdated' ? 'Update' : 'Repair'}
        </Button>
      )}
    </span>
  )
}
