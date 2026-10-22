/** Shared facts about the backend's history tiers, and the graph's scale. */

import type { UsageRange } from '@/lib/types'

/**
 * Seconds covered by one sample, per tier. Mirrors the roll-up in
 * `src-tauri/src/core/usage/history.rs` - samples are bytes-per-bucket, so
 * this is what converts them to a rate.
 */
export const BUCKET_SECONDS: Record<UsageRange, number> = {
  fiveMinutes: 1,
  threeHours: 60,
  day: 60,
  week: 3600,
  month: 3600,
}

/** Seconds each range spans, for drawing an axis before any samples exist. */
export const RANGE_SECONDS: Record<UsageRange, number> = {
  fiveMinutes: 300,
  threeHours: 3 * 3600,
  day: 86_400,
  week: 7 * 86_400,
  month: 30 * 86_400,
}

const KB = 1024
const MB = 1024 * KB

/** Fixed y-axis ceilings, so a quiet period does not zoom noise to full height. */
export const SCALE_OPTIONS = [
  { label: '100 KB/s', value: 100 * KB },
  { label: '500 KB/s', value: 500 * KB },
  { label: '1 MB/s', value: MB },
  { label: '5 MB/s', value: 5 * MB },
  { label: '20 MB/s', value: 20 * MB },
  { label: '100 MB/s', value: 100 * MB },
] as const

/** Multiples of a power of ten that make a readable scale label. */
const NICE_FACTORS = [1, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10] as const

/**
 * The automatic ceiling: a round figure a little above the peak, in KB or
 * MB, so the label on the top guide line reads "600 KB/s" rather than
 * "587.3 KB/s", and the tallest hill never touches the top.
 */
export function niceCeiling(peakBytesPerSecond: number): number {
  if (!Number.isFinite(peakBytesPerSecond) || peakBytesPerSecond <= 0) return 10 * KB

  const target = peakBytesPerSecond * 1.15
  const unit = target >= MB ? MB : KB
  const value = Math.max(1, target / unit)
  const magnitude = 10 ** Math.floor(Math.log10(value))
  const factor = NICE_FACTORS.find((candidate) => candidate * magnitude >= value) ?? 10
  const nice = factor * magnitude
  // "1000 KB/s" is a megabyte that forgot to say so.
  return unit === KB && nice >= 1000 ? MB : nice * unit
}

/** One point of a down/up series, in whatever unit the chart draws. */
export interface SeriesPoint {
  t: number
  down: number
  up: number
}

/**
 * Drops a series to zero across stretches with no samples - the app closed,
 * the PC asleep - so a smooth curve does not draw a hill through time nothing
 * was recorded. Every running second yields a sample, so a missing bucket
 * means not recording; a single one is let through as jitter.
 */
export function withGaps(points: readonly SeriesPoint[], bucketSeconds: number): SeriesPoint[] {
  const out: SeriesPoint[] = []
  points.forEach((point, index) => {
    const previous = points[index - 1]
    if (previous && point.t - previous.t > bucketSeconds * 2.5) {
      out.push(
        { t: previous.t + bucketSeconds, down: 0, up: 0 },
        { t: point.t - bucketSeconds, down: 0, up: 0 },
      )
    }
    out.push(point)
  })
  return out
}
