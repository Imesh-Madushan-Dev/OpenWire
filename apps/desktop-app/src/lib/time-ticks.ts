/**
 * Time-axis ticks for the graphs.
 *
 * Recharts' number axis picks "nice" numbers, which for Unix seconds means
 * ticks at arbitrary instants like 1761136213. These land on the boundaries a
 * person reads a clock by - whole minutes, hours, local midnights - so a label
 * says "6:23:00 PM", not "6:23:13 PM". Pure, so it is easy to test.
 */

const MINUTE = 60
const HOUR = 3600
const DAY = 86_400

/** Candidate spacings, finest first. */
const STEPS = [
  1, 2, 5, 10, 15, 30,
  MINUTE, 2 * MINUTE, 5 * MINUTE, 10 * MINUTE, 15 * MINUTE, 30 * MINUTE,
  HOUR, 2 * HOUR, 3 * HOUR, 4 * HOUR, 6 * HOUR, 12 * HOUR,
  DAY, 2 * DAY, 7 * DAY,
] as const

/** The finest spacing that keeps the count at or under `maxTicks`. */
export function tickStep(spanSeconds: number, maxTicks: number): number {
  const span = Math.max(1, spanSeconds)
  return STEPS.find((step) => span / step <= Math.max(1, maxTicks)) ?? 14 * DAY
}

/**
 * Ticks inside `[from, to]`, on local wall-clock boundaries of the chosen step.
 *
 * Positions are built from each local day's fields - midnight plus so many
 * seconds of wall-clock time - rather than by adding seconds to an instant,
 * so an hourly axis reads 1 AM, 2 AM in any timezone and still says 6 AM on
 * the day the clocks change. Day steps walk calendar days for the same reason.
 */
export function timeTicks(from: number, to: number, maxTicks = 6): number[] {
  if (!Number.isFinite(from) || !Number.isFinite(to) || to <= from) return []

  const step = tickStep(to - from, maxTicks)
  const ticks: number[] = []

  if (step >= DAY) {
    const days = step / DAY
    const cursor = new Date(from * 1000)
    cursor.setHours(0, 0, 0, 0)
    if (cursor.getTime() / 1000 < from) cursor.setDate(cursor.getDate() + 1)
    while (cursor.getTime() / 1000 <= to) {
      ticks.push(cursor.getTime() / 1000)
      cursor.setDate(cursor.getDate() + days)
    }
    return ticks
  }

  const day = new Date(from * 1000)
  day.setHours(0, 0, 0, 0)
  while (day.getTime() / 1000 <= to) {
    const [year, month, date] = [day.getFullYear(), day.getMonth(), day.getDate()]
    // Start near `from` rather than at midnight: a five-minute axis on
    // one-second steps must not walk the whole day. The hour of slack covers
    // a DST shift earlier the same day.
    const offset = Math.max(0, from - day.getTime() / 1000 - HOUR)
    for (let k = Math.floor(offset / step); k * step < DAY; k++) {
      const tick = new Date(year, month, date, 0, 0, k * step).getTime() / 1000
      if (tick > to) break
      // A wall-clock time skipped by DST resolves onto the next one; keep one.
      if (tick >= from && tick !== ticks.at(-1)) ticks.push(tick)
    }
    day.setDate(day.getDate() + 1)
  }
  return ticks
}

/** A label with only the precision the step resolves. */
export function formatTick(unixSeconds: number, step: number): string {
  const date = new Date(unixSeconds * 1000)
  if (step >= DAY) {
    return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
  }
  if (step < MINUTE) {
    return date.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit', second: '2-digit' })
  }
  return date.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })
}
