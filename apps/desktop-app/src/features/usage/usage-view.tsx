/** Usage: totals for a period and the ranked breakdown behind them. */

import { useMemo, useState } from 'react'
import {
  ComputerIcon,
  GlobalIcon,
  Route01Icon,
} from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'

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
import { UsageDonut } from './components/usage-donut'
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
import { useDeviceApps, useSummary, useUsageSpan } from './use-usage'

const DAY = 86_400

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

  // Device-wide per-app figures for exactly the days the summary covers, read
  // from Windows' own accounting rather than from the relay.
  const device = useDeviceApps(summary.fromTs, summary.toTs)

  // The relay only ever sees proxy-aware apps, so it is the fallback, not the
  // source: Windows' table covers everything, including apps that ignore the
  // system proxy entirely.
  const apps = device.entries.length > 0 ? device.entries : summary.apps
  const appsAreDeviceWide = device.entries.length > 0

  // An empty column has two very different causes, and "Recorded while the VPN
  // is connected" reads as a fault rather than as the reason when the real
  // answer is simply that nothing was tunnelled in this period.
  const attributionHint = !summary.attributionSupported
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

      <div className="min-h-0 flex-1 overflow-y-auto px-8 pb-8">
        <motion.div
          variants={staggerParent}
          initial="initial"
          animate="animate"
          className="flex flex-col gap-4"
        >
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-[260px_1fr]">
            <UsageDonut summary={summary} />

            <div className="grid min-w-0 grid-cols-1 gap-4 sm:grid-cols-3">
              <BreakdownColumn
                title="Apps"
                icon={ComputerIcon}
                entries={apps}
                loading={device.loading}
                note={appsAreDeviceWide ? 'Every app on this device' : undefined}
                emptyHint={attributionHint}
              />
              <BreakdownColumn
                title="Hosts"
                icon={GlobalIcon}
                entries={summary.hosts}
                note="Through Maizy only"
                emptyHint={attributionHint}
              />
              <BreakdownColumn
                title="Traffic type"
                icon={Route01Icon}
                entries={summary.kinds}
                note="Through Maizy only"
                emptyHint={attributionHint}
              />
            </div>
          </div>

          <p className="px-1 text-xs leading-relaxed text-muted-foreground">
            {appsAreDeviceWide
              ? "Apps comes from Windows' own per-app accounting, so it covers everything on this device. Hosts and traffic type can only be read from traffic that went through Maizy, because Windows records which app used the network but not where it connected."
              : 'The ring covers all network activity on this device. The three columns cover only traffic that went through Maizy, which is the only traffic it can attribute without a kernel driver.'}{' '}
            History is kept for {span?.retentionDays ?? 90} days.
          </p>
        </motion.div>
      </div>
    </div>
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
