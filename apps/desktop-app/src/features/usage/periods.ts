/**
 * Periods the usage page can summarise.
 *
 * Calendar maths lives here, in JS, because `Date` already knows the host's
 * timezone, month lengths and month names. The backend owns the *other* half -
 * which local day a given instant belongs to - and the two meet on timestamps
 * rather than on date strings, so neither has to reimplement the other.
 *
 * Every boundary is midday, matching `netstat::day_noon_unix`: a DST shift
 * inside the window can then never push a bound onto the adjacent day.
 */

import type { UsageRange, UsageSpan } from '@/lib/types'

/** An explicit span: a calendar month, or dates the user picked. */
export interface SpanPeriod {
  kind: 'span'
  fromTs: number
  toTs: number
  label: string
}

export type Period =
  /** Counted back from today by the backend's own tiers. */
  | { kind: 'rolling'; range: UsageRange; label: string }
  | SpanPeriod

/** Rolling presets, counted back from today. Order matters: the view indexes them. */
export const ROLLING: Period[] = [
  { kind: 'rolling', range: 'day', label: 'Today' },
  { kind: 'rolling', range: 'week', label: 'Last 7 days' },
  { kind: 'rolling', range: 'month', label: 'Last 30 days' },
]

/** Stable value for the picker; also what distinguishes one span from another. */
export function periodKey(period: Period): string {
  return period.kind === 'rolling'
    ? `rolling:${period.range}`
    : `span:${period.fromTs}:${period.toTs}`
}

function noonTs(year: number, month: number, day: number): number {
  return Math.floor(new Date(year, month, day, 12, 0, 0, 0).getTime() / 1000)
}

/** The calendar month we are in now, month-to-date. */
export function currentMonth(now = new Date()): SpanPeriod {
  const year = now.getFullYear()
  const month = now.getMonth()
  return {
    kind: 'span',
    fromTs: noonTs(year, month, 1),
    toTs: noonTs(year, month + 1, 0),
    label: now.toLocaleDateString(undefined, { month: 'long', year: 'numeric' }),
  }
}

/**
 * One entry per calendar month that has data, newest first.
 *
 * The current month is always first, with or without records behind it: it is
 * the default selection, and a fresh install would otherwise open the Month
 * view on an empty list.
 */
export function monthPeriods(span: UsageSpan, now = new Date()): SpanPeriod[] {
  const months = recordedMonths(span)
  const current = currentMonth(now)

  return months.some((entry) => entry.fromTs === current.fromTs)
    ? months
    : [current, ...months]
}

function recordedMonths(span: UsageSpan): SpanPeriod[] {
  if (span.firstTs === null || span.lastTs === null) return []

  const last = new Date(span.lastTs * 1000)
  const cursor = new Date(span.firstTs * 1000)
  cursor.setDate(1)
  cursor.setHours(12, 0, 0, 0)

  const out: SpanPeriod[] = []
  while (cursor <= last && out.length <= 24) {
    const year = cursor.getFullYear()
    const month = cursor.getMonth()
    out.push({
      kind: 'span',
      fromTs: noonTs(year, month, 1),
      // Day 0 of the next month is the last day of this one, which is how the
      // leap-year and 30/31-day cases take care of themselves.
      toTs: noonTs(year, month + 1, 0),
      label: cursor.toLocaleDateString(undefined, { month: 'long', year: 'numeric' }),
    })
    cursor.setMonth(month + 1)
  }

  return out.reverse()
}

/** `YYYY-MM-DD` in local time, for an `<input type="date">` value. */
export function toDateInput(ts: number): string {
  const date = new Date(ts * 1000)
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  return `${date.getFullYear()}-${month}-${day}`
}

/**
 * `YYYY-MM-DD` back to a timestamp at local midday.
 *
 * Parsed by hand rather than with `new Date(value)`, which reads a bare date as
 * UTC and would land on the previous day for anyone west of Greenwich.
 */
export function fromDateInput(value: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value)
  if (!match) return null

  const [, year, month, day] = match
  const ts = noonTs(Number(year), Number(month) - 1, Number(day))
  return Number.isFinite(ts) ? ts : null
}

/** Human label for a resolved span, e.g. `12 Sep 2026 - 18 Sep 2026`. */
export function spanLabel(fromTs: number, toTs: number): string {
  const format = (ts: number) =>
    new Date(ts * 1000).toLocaleDateString(undefined, {
      day: 'numeric',
      month: 'short',
      year: 'numeric',
    })

  const from = format(fromTs)
  const to = format(toTs)
  return from === to ? from : `${from} – ${to}`
}
