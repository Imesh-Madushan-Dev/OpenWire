import { describe, expect, test } from 'bun:test'

import { formatTick, tickStep, timeTicks } from '../src/lib/time-ticks'

const at = (...parts: [number, number, number, number?, number?, number?]) => {
  const [year, month, day, hour = 0, minute = 0, second = 0] = parts
  return new Date(year, month, day, hour, minute, second).getTime() / 1000
}

describe('tickStep', () => {
  test('picks the finest step that stays under the cap', () => {
    expect(tickStep(300, 6)).toBe(60)
    expect(tickStep(3 * 3600, 7)).toBe(30 * 60)
    expect(tickStep(86_400, 7)).toBe(4 * 3600)
    expect(tickStep(7 * 86_400, 7)).toBe(86_400)
  })

  test('never divides by a zero or negative span', () => {
    expect(tickStep(0, 6)).toBe(1)
    expect(tickStep(-50, 0)).toBe(1)
  })
})

describe('timeTicks', () => {
  test('lands on whole local minutes inside the range', () => {
    const from = at(2026, 9, 22, 18, 20, 13)
    const ticks = timeTicks(from, from + 300, 6)

    expect(ticks.length).toBeGreaterThan(0)
    expect(ticks.length).toBeLessThanOrEqual(6)
    for (const tick of ticks) {
      expect(tick).toBeGreaterThanOrEqual(from)
      expect(tick).toBeLessThanOrEqual(from + 300)
      expect(new Date(tick * 1000).getSeconds()).toBe(0)
    }
  })

  test('aligns hourly steps to the local day, not the epoch', () => {
    const from = at(2026, 9, 22, 0, 30)
    const ticks = timeTicks(from, at(2026, 9, 23, 0, 30), 7)

    expect(ticks.map((tick) => new Date(tick * 1000).getHours())).toEqual([4, 8, 12, 16, 20, 0])
  })

  test('steps whole days on local midnights', () => {
    const ticks = timeTicks(at(2026, 9, 18, 12), at(2026, 9, 25, 12), 7)

    expect(ticks).toHaveLength(7)
    ticks.forEach((tick, index) => {
      const date = new Date(tick * 1000)
      expect(date.getHours()).toBe(0)
      expect(date.getDate()).toBe(19 + index)
    })
  })

  test('is empty for an empty or inverted range', () => {
    expect(timeTicks(100, 100)).toEqual([])
    expect(timeTicks(200, 100)).toEqual([])
    expect(timeTicks(Number.NaN, 100)).toEqual([])
  })
})

describe('formatTick', () => {
  test('shows seconds only for sub-minute steps', () => {
    const tick = at(2026, 9, 22, 18, 22, 48)
    expect(formatTick(tick, 10)).toContain('48')
    expect(formatTick(tick, 60)).not.toContain('48')
  })

  test('shows the date for day steps', () => {
    expect(formatTick(at(2026, 9, 22), 86_400)).toContain('22')
  })
})
