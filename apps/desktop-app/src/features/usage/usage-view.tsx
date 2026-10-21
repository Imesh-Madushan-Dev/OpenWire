/**
 * Usage: totals for a period and the ranked breakdown behind them, for the
 * whole device or only what went through Maizy.
 */

import { useMemo, useState } from 'react'
import { ComputerIcon, GlobalIcon, Route01Icon } from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'
import { cn } from 'cn'

import { Segmented } from '@/components/segmented'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { staggerParent } from '@/lib/motion'

import { BreakdownColumn } from './components/breakdown-column'
import { MonitorPrompt } from './components/monitor-prompt'
import { UsageDonut, type UsageScope } from './components/usage-donut'
import {
  currentMonth,
  fromDateInput,
  monthPeriods,
  periodKey,
  ROLLING,
  spanLabel,
  toDateInput,
  type Period,
} from './periods'
import { useMonitor } from './use-monitor'
import { useAppIcons, useSummary, useUsageSpan, useWindowsApps } from './use-usage'

const DAY = 86_400

const SCOPES = [
  { value: 'all' as const, label: 'All traffic' },
  { value: 'maizy' as const, label: 'Through Maizy' },
]

/** Remembered per viewer: someone who checks only their VPN traffic should
 *  not have to switch every visit. Storage can be blocked; the default holds. */
const SCOPE_KEY = 'maizy.usage.scope'

function readScope(): UsageScope {
  try {
    return localStorage.getItem(SCOPE_KEY) === 'maizy' ? 'maizy' : 'all'
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

/**
 * What the right-hand picker chooses. `month` and `custom` are modes rather
 * than periods: each hands the actual choice to its own control in the centre
 * of the header, so one dropdown never has to hold both presets and a calendar.
 */
type Mode = 'today' | 'week' | 'rolling30' | 'month' | 'custom'

const MODE_LABELS: Record<Mode, string> = {
  today: 'Today',
  week: 'Last 7 days',
  rolling30: 'Last 30 days',
  month: 'Month',
  custom: 'Custom range',
}

const MODE_ORDER: Mode[] = ['today', 'week', 'rolling30', 'month', 'custom']

const ROLLING_FOR: Record<'today' | 'week' | 'rolling30', Period> = {
  today: ROLLING[0],
  week: ROLLING[1],
  rolling30: ROLLING[2],
}

export function UsageView() {
  const span = useUsageSpan()
  // Opens on the current month, the way GlassWire does.
  const [mode, setMode] = useState<Mode>('month')
  const [month, setMonth] = useState<string | null>(null)

  // Read once per mount rather than per render: the date pickers only need it
  // for their bounds and their initial value.
  // ponytail: a session left open past midnight keeps yesterday's `max`. The
  // backend clamps anyway; revisit if anyone actually trips over it.
  const [today] = useState(() => Math.floor(Date.now() / 1000))

  // Defaults to the last week, so opening Custom shows real figures rather
  // than an empty panel waiting on two inputs.
  const [customFrom, setCustomFrom] = useState(() => toDateInput(today - 6 * DAY))
  const [customTo, setCustomTo] = useState(() => toDateInput(today))

  // The current month is always present, so Month mode works before any
  // history exists and lands on this month by default.
  const months = useMemo(
    () => (span ? monthPeriods(span) : [currentMonth()]),
    [span],
  )

  // Key -> label, so the trigger reads "September 2026" rather than the
  // `span:...:...` value its items are addressed by.
  const monthLabels = useMemo(() => {
    const map: Record<string, string> = {}
    for (const entry of months) map[periodKey(entry)] = entry.label
    return map
  }, [months])

  // `months` is newest-first with the current month at its head, so the
  // fallback is this month without having to search for it.
  const activeMonth = months.find((entry) => periodKey(entry) === month) ?? months[0]

  const period = useMemo<Period>(() => {
    if (mode === 'month') return activeMonth

    if (mode === 'custom') {
      const fromTs = fromDateInput(customFrom)
      const toTs = fromDateInput(customTo)
      // An incomplete pair falls back to today rather than querying a nonsense
      // window; an inverted one is simply read in the order it makes sense.
      if (fromTs === null || toTs === null) return ROLLING[0]
      return {
        kind: 'span',
        fromTs: Math.min(fromTs, toTs),
        toTs: Math.max(fromTs, toTs),
        label: MODE_LABELS.custom,
      }
    }

    return ROLLING_FOR[mode]
  }, [mode, activeMonth, customFrom, customTo])

  const summary = useSummary(period)
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
  const columns = scope === 'maizy' ? summary.maizy : device
  const fallback = scope === 'all' && !device

  // Seconds to answer, so only asked for when it is actually what is shown.
  const windows = useWindowsApps(summary.fromTs, summary.toTs, fallback)
  const apps = columns?.apps ?? windows.ranked
  const iconFor = useAppIcons(apps.entries.map((entry) => entry.key))

  // An empty Maizy column has two very different causes, and "nothing was
  // recorded" reads as a fault when the real answer is that nothing went
  // through Maizy in this period.
  const emptyHint =
    scope === 'all'
      ? 'Nothing was recorded in this period.'
      : !summary.attributionSupported
        ? 'Per-app names need Windows.'
        : summary.tunnelUp + summary.tunnelDown === 0
          ? 'No traffic went through Maizy in this period. Connect, or turn on "Measure usage while disconnected" in Settings.'
          : 'Recorded while the VPN is connected.'

  // The dates the backend actually covered, which is not always what was asked
  // for: a range reaching past retention comes back trimmed.
  const covered = summary.fromTs > 0 ? spanLabel(summary.fromTs, summary.toTs) : null

  const earliest = span?.firstTs ? toDateInput(span.firstTs) : undefined
  const latest = toDateInput(today)

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Three zones: the centre one stays centred on the page regardless of
          how wide the title or the right-hand picker are. */}
      <header className="grid shrink-0 grid-cols-[1fr_auto_1fr] items-center gap-3 px-8 pt-1 pb-4">
        <div className="min-w-0">
          <h1 className="font-heading text-xl font-semibold">Usage</h1>
          <p className="truncate text-xs text-muted-foreground">
            {covered ?? 'Where your bandwidth went.'}
          </p>
        </div>

        <div className="flex items-center justify-center gap-1.5">
          {mode === 'month' && (
            <Select
              items={monthLabels}
              value={periodKey(activeMonth)}
              onValueChange={(value) => setMonth(value)}
            >
              <SelectTrigger
                size="sm"
                aria-label="Month"
                className="min-w-40 justify-center text-xs"
              >
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {months.map((entry) => (
                  <SelectItem key={periodKey(entry)} value={periodKey(entry)}>
                    {entry.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}

          {mode === 'custom' && (
            <>
              <DateField
                label="From"
                value={customFrom}
                min={earliest}
                max={latest}
                onChange={setCustomFrom}
              />
              <span aria-hidden className="text-xs text-muted-foreground">
                &ndash;
              </span>
              <DateField
                label="To"
                value={customTo}
                min={earliest}
                max={latest}
                onChange={setCustomTo}
              />
            </>
          )}
        </div>

        <div className="flex justify-end">
          <Select
            items={MODE_LABELS}
            value={mode}
            onValueChange={(value) => setMode((value as Mode | null) ?? 'today')}
          >
            <SelectTrigger size="sm" aria-label="Usage period" className="min-w-30 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {MODE_ORDER.map((value) => (
                <SelectItem key={value} value={value}>
                  {MODE_LABELS[value]}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </header>

      <div className="flex shrink-0 items-center justify-between gap-4 px-8 pb-3">
        <Segmented
          options={SCOPES}
          value={scope}
          onChange={setScope}
          layoutId="usage-scope"
          size="sm"
          aria-label="Traffic shown"
        />
        <div className="flex items-center gap-4">
          {scope === 'all' && <MonitorBadge monitor={monitor} />}
          <Legend />
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-8 pb-8">
        {/* Keyed by scope so a switch replays the entrance and every column
            starts collapsed again, instead of morphing one list into another. */}
        <motion.div
          key={scope}
          variants={staggerParent}
          initial="initial"
          animate="animate"
          className="flex flex-col gap-4"
        >
          {/* One surface, four zones side by side at every window size - the
              ring, then the three rankings - divided by grooves rather than
              split into cards. Stacking them pushed the figures out of view. */}
          <div className="panel grid grid-cols-[14rem_repeat(3,minmax(0,1fr))] divide-x divide-hairline overflow-hidden rounded-2xl">
            <UsageDonut summary={summary} scope={scope} />

            <BreakdownColumn
              title="Apps"
              icon={ComputerIcon}
              ranked={apps}
              loading={fallback && windows.loading}
              note={fallback ? "From Windows' per-app records" : undefined}
              emptyHint={emptyHint}
              iconFor={iconFor}
            />

            {columns ? (
              <>
                <BreakdownColumn
                  title="Hosts"
                  icon={GlobalIcon}
                  ranked={columns.hosts}
                  emptyHint={emptyHint}
                  flags
                />
                <BreakdownColumn
                  title="Traffic type"
                  icon={Route01Icon}
                  ranked={columns.kinds}
                  emptyHint={emptyHint}
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
          </div>

          <p className="px-1 text-xs leading-relaxed text-muted-foreground">
            {footnote(scope, summary.fromTs, summary.monitoredFromTs, Boolean(device))} History
            is kept for {span?.retentionDays ?? 90} days.
          </p>
        </motion.div>
      </div>
    </div>
  )
}

function footnote(
  scope: UsageScope,
  fromTs: number,
  monitoredFromTs: number | null,
  hasDevice: boolean,
): string {
  if (scope === 'maizy') {
    return 'Only traffic that went through Maizy: over the VPN, or routed for measuring while disconnected.'
  }
  if (!hasDevice) {
    return "The ring covers all network activity on this device, and Apps comes from Windows' own per-app records. Hosts and traffic types need detailed monitoring."
  }
  // The period began before monitoring did, so its first days are missing.
  const partial = monitoredFromTs !== null && monitoredFromTs > fromTs
  const since = partial
    ? ` since ${new Date(monitoredFromTs * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short' })}`
    : ''
  return `Every app, host and traffic type on this device${since}, recorded by detailed monitoring. Traffic Maizy carried is credited to the app that sent it.`
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
    <span className={cn('flex items-center gap-1.5 text-[11px] font-medium', tone.text)}>
      <span className="relative flex size-1.5">
        {state === 'active' && (
          <span
            aria-hidden
            className="absolute inset-0 animate-ping rounded-full bg-success opacity-60"
          />
        )}
        <span className={cn('relative size-1.5 rounded-full', tone.dot)} />
      </span>
      {tone.label}
      {troubled && (
        <Button
          variant="outline"
          size="xs"
          className="ml-1 h-5 px-2 text-[11px]"
          disabled={busy}
          onClick={() => void monitor.enable()}
        >
          {busy ? 'Waiting for Windows…' : state === 'outdated' ? 'Update' : 'Repair'}
        </Button>
      )}
    </span>
  )
}

function Legend() {
  return (
    <span className="flex items-center gap-3 text-[11px] text-muted-foreground">
      <span className="flex items-center gap-1.5">
        <span aria-hidden className="h-1.5 w-3 rounded-full bg-chart-down" />
        Incoming
      </span>
      <span className="flex items-center gap-1.5">
        <span aria-hidden className="h-1.5 w-3 rounded-full bg-chart-up" />
        Outgoing
      </span>
    </span>
  )
}

function DateField({
  label,
  value,
  min,
  max,
  onChange,
}: {
  label: string
  value: string
  min?: string
  max: string
  onChange: (value: string) => void
}) {
  return (
    <Input
      type="date"
      aria-label={label}
      value={value}
      min={min}
      max={max}
      onChange={(event) => onChange(event.target.value)}
      // `color-scheme` is what makes the native picker and its calendar icon
      // match the theme; without it Chromium paints a white widget on the panel.
      className="h-8 w-33 px-2.5 text-xs scheme-light dark:scheme-dark"
    />
  )
}
