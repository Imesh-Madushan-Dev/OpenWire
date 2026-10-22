import { afterAll, beforeAll, describe, expect, test } from 'bun:test'

import { timeTicks } from '../src/lib/time-ticks'

// Bun re-reads TZ on change, so each block can pin the zone it is about.
const original = process.env.TZ

function inZone(zone: string) {
  beforeAll(() => {
    process.env.TZ = zone
  })
  afterAll(() => {
    process.env.TZ = original
  })
}

const at = (year: number, month: number, day: number, hour = 0) =>
  new Date(year, month, day, hour).getTime() / 1000

describe('on the day the clocks go forward (New York, 8 March 2026)', () => {
  inZone('America/New_York')

  test('hourly ticks keep to the wall clock', () => {
    const ticks = timeTicks(at(2026, 2, 8), at(2026, 2, 9), 7)
    expect(ticks.map((tick) => new Date(tick * 1000).getHours())).toEqual([0, 4, 8, 12, 16, 20, 0])
  })
})

describe('on the day the clocks go back (New York, 1 November 2026)', () => {
  inZone('America/New_York')

  test('hourly ticks keep to the wall clock, without repeats', () => {
    const ticks = timeTicks(at(2026, 10, 1), at(2026, 10, 2), 7)
    expect(ticks.map((tick) => new Date(tick * 1000).getHours())).toEqual([0, 4, 8, 12, 16, 20, 0])
    expect(new Set(ticks).size).toBe(ticks.length)
  })
})

describe('in a half-hour timezone (Colombo, +5:30)', () => {
  inZone('Asia/Colombo')

  test('hourly ticks land on whole local hours', () => {
    const ticks = timeTicks(at(2026, 9, 22, 0), at(2026, 9, 23, 0), 7)
    expect(ticks.length).toBeGreaterThan(0)
    for (const tick of ticks) expect(new Date(tick * 1000).getMinutes()).toBe(0)
  })
})
