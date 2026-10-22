import { describe, expect, test } from 'bun:test'

import {
  customPeriod,
  periodBounds,
  periodDays,
  periodFor,
  periodKey,
  shiftAnchor,
  subPeriod,
} from '../src/features/usage/periods'
import { niceCeiling, withGaps } from '../src/features/usage/tiers'

const KB = 1024
const MB = 1024 * KB
const day = (year: number, month: number, date: number) => new Date(year, month, date)
const localDate = (unixSeconds: number) => new Date(unixSeconds * 1000)
const now = day(2026, 9, 22)

describe('niceCeiling', () => {
  test('falls back to a small scale when there is no traffic', () => {
    expect(niceCeiling(0)).toBe(10 * KB)
    expect(niceCeiling(-5)).toBe(10 * KB)
    expect(niceCeiling(Number.NaN)).toBe(10 * KB)
  })

  test('rounds up to a readable figure with headroom', () => {
    expect(niceCeiling(600 * KB)).toBe(800 * KB)
    expect(niceCeiling(90 * KB)).toBe(150 * KB)
    expect(niceCeiling(2 * MB)).toBe(2.5 * MB)
  })

  test('never labels a megabyte as a thousand kilobytes', () => {
    expect(niceCeiling(880 * KB)).toBe(MB)
    expect(niceCeiling(900 * KB)).toBe(1.5 * MB)
  })

  test('always clears the peak', () => {
    for (let peak = 37; peak < 500 * MB; peak *= 1.7) {
      expect(niceCeiling(peak)).toBeGreaterThan(peak)
    }
  })
})

describe('periodFor', () => {
  test('a day is one day, labelled without this year', () => {
    const period = periodFor('day', day(2026, 9, 22), 0, now)
    expect(period.fromTs).toBe(period.toTs)
    expect(localDate(period.fromTs).getHours()).toBe(12)
    expect(period.label).toContain('22')
    expect(period.label).not.toContain('2026')
  })

  test('a week starts on the given weekday', () => {
    // 22 October 2026 is a Thursday.
    const sunday = periodFor('week', day(2026, 9, 22), 0, now)
    expect(localDate(sunday.fromTs).getDate()).toBe(18)
    expect(localDate(sunday.toTs).getDate()).toBe(24)

    const monday = periodFor('week', day(2026, 9, 22), 1, now)
    expect(localDate(monday.fromTs).getDate()).toBe(19)
    expect(localDate(monday.toTs).getDate()).toBe(25)
  })

  test('a month runs from the 1st to its last day, leap years included', () => {
    const february = periodFor('month', day(2028, 1, 10), 0, now)
    expect(localDate(february.fromTs).getDate()).toBe(1)
    expect(localDate(february.toTs).getDate()).toBe(29)
    expect(periodDays(february)).toHaveLength(29)
    expect(february.label).toContain('2028')
  })
})

describe('shiftAnchor', () => {
  test('steps by the mode', () => {
    expect(shiftAnchor('day', day(2026, 9, 22), -1).getDate()).toBe(21)
    expect(shiftAnchor('week', day(2026, 9, 22), 1).getDate()).toBe(29)
  })

  test('steps months from the 1st, so the 31st does not skip a month', () => {
    const next = shiftAnchor('month', day(2026, 0, 31), 1)
    expect(next.getMonth()).toBe(1)
    expect(next.getDate()).toBe(1)
  })
})

describe('custom periods', () => {
  test('either pick order gives the same run', () => {
    const forward = customPeriod(day(2026, 9, 3), day(2026, 9, 22), now)
    const backward = customPeriod(day(2026, 9, 22), day(2026, 9, 3), now)
    expect(periodKey(forward)).toBe(periodKey(backward))
    expect(periodDays(forward)).toHaveLength(20)
  })

  test('a sub-period is the chosen days of its parent', () => {
    const week = periodFor('week', day(2026, 9, 22), 0, now)
    const days = periodDays(week)
    const middle = subPeriod(days, 2, 4, now)
    expect(middle.fromTs).toBe(days[2])
    expect(middle.toTs).toBe(days[4])
  })
})

describe('periodBounds', () => {
  test('spans local midnight to the midnight after the last day', () => {
    const week = periodFor('week', day(2026, 9, 22), 0, now)
    const [from, to] = periodBounds(week)
    expect(localDate(from).getHours()).toBe(0)
    expect(localDate(from).getDate()).toBe(18)
    expect(localDate(to).getHours()).toBe(0)
    expect(localDate(to).getDate()).toBe(25)
  })
})

describe('withGaps', () => {
  const point = (t: number, value = 5) => ({ t, down: value, up: value })

  test('leaves an unbroken series alone', () => {
    const series = [point(0), point(60), point(120)]
    expect(withGaps(series, 60)).toEqual(series)
  })

  test('lets a single missing bucket through as jitter', () => {
    const series = [point(0), point(120)]
    expect(withGaps(series, 60)).toEqual(series)
  })

  test('drops to zero across a stretch nothing was recorded', () => {
    const filled = withGaps([point(0), point(3600 * 9), point(3600 * 10)], 3600)
    expect(filled.map((entry) => entry.t)).toEqual([0, 3600, 3600 * 8, 3600 * 9, 3600 * 10])
    expect(filled[1]).toEqual({ t: 3600, down: 0, up: 0 })
    expect(filled[2]).toEqual({ t: 3600 * 8, down: 0, up: 0 })
  })

  test('handles empty and single-point series', () => {
    expect(withGaps([], 60)).toEqual([])
    expect(withGaps([point(10)], 60)).toEqual([point(10)])
  })
})
