/**
 * Periods the usage page can summarise: GlassWire's Day, Week and Month, each
 * anchored on a calendar day the user picks, plus a custom run of days.
 *
 * Calendar maths lives here, in JS, because `Date` already knows the host's
 * timezone, month lengths and month names. The backend owns the *other* half -
 * which local day a given instant belongs to - and the two meet on timestamps
 * rather than on date strings, so neither has to reimplement the other.
 *
 * Every summary bound is midday, matching `netstat::day_noon_unix`: a DST
 * shift inside the window can then never push a bound onto the adjacent day.
 */

export type PeriodMode = 'day' | 'week' | 'month' | 'custom'

/** A run of whole local days. */
export interface Period {
  /** Midday of the first and last day. */
  fromTs: number
  toTs: number
  label: string
}

/** Stable identity: what distinguishes one period from another. */
export function periodKey(period: Period): string {
  return `${period.fromTs}:${period.toTs}`
}

function noon(date: Date): number {
  return Math.floor(
    new Date(date.getFullYear(), date.getMonth(), date.getDate(), 12).getTime() / 1000,
  )
}

/** The local day a date falls on, at midnight. */
export function startOfDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate())
}

function addDays(date: Date, days: number): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days)
}

/**
 * The locale's first day of the week, 0 for Sunday. Read from `Intl` where
 * the engine knows it, so the week periods and the calendar agree with what
 * the person expects; Sunday otherwise, as GlassWire's calendar has it.
 */
export function firstDayOfWeek(locale?: string): number {
  try {
    const info = new Intl.Locale(locale ?? navigator.language) as Intl.Locale & {
      getWeekInfo?: () => { firstDay: number }
      weekInfo?: { firstDay: number }
    }
    const firstDay = info.getWeekInfo?.().firstDay ?? info.weekInfo?.firstDay
    // `Intl` counts Monday as 1 and Sunday as 7.
    return firstDay === undefined ? 0 : firstDay % 7
  } catch {
    return 0
  }
}

/** The day, week or month containing `anchor`. */
export function periodFor(
  mode: Exclude<PeriodMode, 'custom'>,
  anchor: Date,
  weekStartsOn = 0,
  now = new Date(),
): Period {
  const day = startOfDay(anchor)

  if (mode === 'day') {
    return { fromTs: noon(day), toTs: noon(day), label: dayLabel(day, now) }
  }

  if (mode === 'week') {
    const start = addDays(day, -((day.getDay() - weekStartsOn + 7) % 7))
    const end = addDays(start, 6)
    return { fromTs: noon(start), toTs: noon(end), label: rangeLabel(start, end, now) }
  }

  const start = new Date(day.getFullYear(), day.getMonth(), 1)
  // Day 0 of the next month is the last day of this one, which is how the
  // leap-year and 30/31-day cases take care of themselves.
  const end = new Date(day.getFullYear(), day.getMonth() + 1, 0)
  return {
    fromTs: noon(start),
    toTs: noon(end),
    label: start.toLocaleDateString(undefined, {
      month: 'long',
      ...(start.getFullYear() === now.getFullYear() ? {} : { year: 'numeric' }),
    }),
  }
}

/** Any run of days, in whichever order the two were picked. */
export function customPeriod(a: Date, b: Date, now = new Date()): Period {
  const [start, end] = a <= b ? [startOfDay(a), startOfDay(b)] : [startOfDay(b), startOfDay(a)]
  return {
    fromTs: noon(start),
    toTs: noon(end),
    label: start.getTime() === end.getTime() ? dayLabel(start, now) : rangeLabel(start, end, now),
  }
}

/** The anchor one period earlier or later. */
export function shiftAnchor(mode: Exclude<PeriodMode, 'custom'>, anchor: Date, step: number): Date {
  if (mode === 'day') return addDays(anchor, step)
  if (mode === 'week') return addDays(anchor, step * 7)
  // From the 1st, so stepping from 31 January lands in February, not March.
  return new Date(anchor.getFullYear(), anchor.getMonth() + step, 1)
}

/** The local days a period covers, as midday timestamps, oldest first. */
export function periodDays(period: Period): number[] {
  const days: number[] = []
  const last = new Date(period.toTs * 1000)
  for (let cursor = startOfDay(new Date(period.fromTs * 1000)); cursor <= last; cursor = addDays(cursor, 1)) {
    days.push(noon(cursor))
  }
  return days
}

/** The instants a period covers - its first midnight to the one after its last day. */
export function periodBounds(period: Pick<Period, 'fromTs' | 'toTs'>): [number, number] {
  const from = startOfDay(new Date(period.fromTs * 1000))
  const to = addDays(startOfDay(new Date(period.toTs * 1000)), 1)
  return [from.getTime() / 1000, to.getTime() / 1000]
}

/** Days `from` to `to` of a period, inclusive, as a period of their own. */
export function subPeriod(days: readonly number[], from: number, to: number, now = new Date()): Period {
  return customPeriod(new Date(days[from] * 1000), new Date(days[to] * 1000), now)
}

/** "October 22", with the year only when it is not this one. */
function dayLabel(day: Date, now: Date): string {
  return day.toLocaleDateString(undefined, {
    month: 'long',
    day: 'numeric',
    ...(day.getFullYear() === now.getFullYear() ? {} : { year: 'numeric' }),
  })
}

/** "Oct 19 – 25", "Oct 26 – Nov 1": the locale's own way of joining two dates. */
function rangeLabel(start: Date, end: Date, now: Date): string {
  const format = new Intl.DateTimeFormat(undefined, {
    month: 'short',
    day: 'numeric',
    ...(start.getFullYear() === now.getFullYear() && end.getFullYear() === now.getFullYear()
      ? {}
      : { year: 'numeric' }),
  })
  return format.formatRange(start, end)
}
