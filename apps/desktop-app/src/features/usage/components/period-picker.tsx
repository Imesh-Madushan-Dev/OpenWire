/**
 * The Usage toolbar's centre: GlassWire's date button - "October 22" - that
 * opens a calendar, with arrows either side to step a period at a time.
 *
 * What the calendar picks depends on the mode: a day; any day of the wanted
 * week, which then lights up whole; a month, from a grid of them; or, in
 * Custom, the first and last day of a run.
 */

import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowLeft01Icon, ArrowRight01Icon } from '@hugeicons/core-free-icons'
import type { DateRange, Matcher } from 'react-day-picker'
import { cn } from 'cn'

import { ToolbarButton } from '@/components/toolbar'
import { Button } from '@/components/ui/button'
import { Calendar } from '@/components/ui/calendar'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'

import {
  customPeriod,
  periodFor,
  shiftAnchor,
  startOfDay,
  type Period,
  type PeriodMode,
} from '../periods'

interface PeriodPickerProps {
  mode: PeriodMode
  /** What the page is showing now. */
  period: Period
  anchor: Date
  onAnchorChange: (anchor: Date) => void
  onCustomChange: (from: Date, to: Date) => void
  /** The first day with records; nothing before it can be picked. */
  earliest: Date | null
  today: Date
  weekStartsOn: number
}

export function PeriodPicker({
  mode,
  period,
  anchor,
  onAnchorChange,
  onCustomChange,
  earliest,
  today,
  weekStartsOn,
}: PeriodPickerProps) {
  const [open, setOpen] = useState(false)

  const stepper = mode !== 'custom'
  const canStep = (step: number) => {
    if (mode === 'custom') return false
    const next = periodFor(mode, shiftAnchor(mode, anchor, step), weekStartsOn, today)
    // Forward only while the period has begun; back only while it has records.
    if (step > 0) return startOfDay(new Date(next.fromTs * 1000)) <= startOfDay(today)
    return earliest === null || startOfDay(new Date(next.toTs * 1000)) >= startOfDay(earliest)
  }
  const step = (by: number) => {
    if (mode !== 'custom') onAnchorChange(shiftAnchor(mode, anchor, by))
  }

  const disabled: Matcher[] = [{ after: today }, ...(earliest ? [{ before: earliest }] : [])]
  const from = new Date(period.fromTs * 1000)
  const to = new Date(period.toTs * 1000)

  return (
    <div className="flex items-center gap-0.5">
      {stepper && (
        <ToolbarButton
          aria-label="Previous period"
          disabled={!canStep(-1)}
          onClick={() => step(-1)}
          className="px-1"
        >
          <HugeiconsIcon icon={ArrowLeft01Icon} size={14} strokeWidth={2} />
        </ToolbarButton>
      )}

      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger
          render={
            <ToolbarButton
              aria-label={`Showing ${period.label}. Choose another`}
              className="min-w-28 justify-center px-3 text-[13px] font-semibold"
            />
          }
        >
          {period.label}
        </PopoverTrigger>
        <PopoverContent align="center" className="w-auto gap-0 p-0">
          {mode === 'month' ? (
            <MonthGrid
              selected={from}
              today={today}
              earliest={earliest}
              onPick={(month) => {
                onAnchorChange(month)
                setOpen(false)
              }}
            />
          ) : mode === 'custom' ? (
            <RangeCalendar
              initial={{ from, to }}
              disabled={disabled}
              weekStartsOn={weekStartsOn}
              today={today}
              onPick={(a, b) => {
                onCustomChange(a, b)
                setOpen(false)
              }}
            />
          ) : (
            <Calendar
              mode="range"
              selected={{ from, to }}
              // Any day stands for its period: the week or day it falls in.
              onSelect={(_range, picked) => {
                onAnchorChange(startOfDay(picked))
                setOpen(false)
              }}
              defaultMonth={from}
              endMonth={today}
              startMonth={earliest ?? undefined}
              disabled={disabled}
              weekStartsOn={toWeekday(weekStartsOn)}
              className="bg-transparent"
            />
          )}
        </PopoverContent>
      </Popover>

      {stepper && (
        <ToolbarButton
          aria-label="Next period"
          disabled={!canStep(1)}
          onClick={() => step(1)}
          className="px-1"
        >
          <HugeiconsIcon icon={ArrowRight01Icon} size={14} strokeWidth={2} />
        </ToolbarButton>
      )}
    </div>
  )
}

/**
 * First click starts the run, second ends it, and the range commits then.
 * `resetOnSelect` makes a click on a complete range start a new one, so a
 * complete range arriving in `onSelect` is always the end of a pick.
 */
function RangeCalendar({
  initial,
  disabled,
  weekStartsOn,
  today,
  onPick,
}: {
  initial: { from: Date; to: Date }
  disabled: Matcher[]
  weekStartsOn: number
  today: Date
  onPick: (from: Date, to: Date) => void
}) {
  const [draft, setDraft] = useState<DateRange | undefined>(initial)

  const preview = draft?.from ? customPeriod(draft.from, draft.to ?? draft.from, today) : null

  return (
    <div className="flex flex-col">
      <Calendar
        mode="range"
        selected={draft}
        resetOnSelect
        onSelect={(range) => {
          setDraft(range)
          if (range?.from && range.to) onPick(range.from, range.to)
        }}
        defaultMonth={initial.to}
        endMonth={today}
        disabled={disabled}
        weekStartsOn={toWeekday(weekStartsOn)}
        className="bg-transparent"
      />
      <div className="flex items-center justify-between gap-3 border-t border-hairline px-3 py-2">
        <span className="text-xs text-muted-foreground">
          {draft?.from && !draft.to ? 'Now pick the last day' : (preview?.label ?? 'Pick the first day')}
        </span>
        <Button
          size="xs"
          disabled={!draft?.from}
          onClick={() => {
            if (draft?.from) onPick(draft.from, draft.to ?? draft.from)
          }}
        >
          Apply
        </Button>
      </div>
    </div>
  )
}

/** A year of months, three by four, stepping a year at a time. */
function MonthGrid({
  selected,
  today,
  earliest,
  onPick,
}: {
  selected: Date
  today: Date
  earliest: Date | null
  onPick: (month: Date) => void
}) {
  const [year, setYear] = useState(selected.getFullYear())
  const firstYear = earliest?.getFullYear() ?? today.getFullYear() - 1

  return (
    <div className="flex w-64 flex-col gap-2 p-3">
      <div className="flex items-center justify-between">
        <ToolbarButton
          aria-label="Previous year"
          disabled={year <= firstYear}
          onClick={() => setYear((value) => value - 1)}
          className="px-1"
        >
          <HugeiconsIcon icon={ArrowLeft01Icon} size={14} strokeWidth={2} />
        </ToolbarButton>
        <span className="numeric text-sm font-semibold">{year}</span>
        <ToolbarButton
          aria-label="Next year"
          disabled={year >= today.getFullYear()}
          onClick={() => setYear((value) => value + 1)}
          className="px-1"
        >
          <HugeiconsIcon icon={ArrowRight01Icon} size={14} strokeWidth={2} />
        </ToolbarButton>
      </div>
      <div role="group" aria-label={`Months of ${year}`} className="grid grid-cols-3 gap-1">
        {Array.from({ length: 12 }, (_, month) => {
          const date = new Date(year, month, 1)
          const lastDay = new Date(year, month + 1, 0)
          const future = date > today
          const beforeRecords = earliest !== null && lastDay < startOfDay(earliest)
          const chosen = year === selected.getFullYear() && month === selected.getMonth()
          return (
            <button
              key={month}
              type="button"
              aria-pressed={chosen}
              disabled={future || beforeRecords}
              onClick={() => onPick(date)}
              className={cn(
                'h-8 rounded-sm text-xs font-medium outline-none transition-colors',
                'focus-visible:ring-2 focus-visible:ring-primary/50 disabled:opacity-35',
                chosen
                  ? 'action-fill bg-primary text-primary-foreground'
                  : 'hover:bg-tint/8 disabled:hover:bg-transparent',
              )}
            >
              {date.toLocaleDateString(undefined, { month: 'short' })}
            </button>
          )
        })}
      </div>
    </div>
  )
}

function toWeekday(day: number): 0 | 1 | 2 | 3 | 4 | 5 | 6 {
  return (((day % 7) + 7) % 7) as 0 | 1 | 2 | 3 | 4 | 5 | 6
}
