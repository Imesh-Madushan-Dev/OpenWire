/** Shared facts about the backend's history tiers. */

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

/** Fixed y-axis ceilings, so a quiet period does not zoom noise to full height. */
export const SCALE_OPTIONS = [
  { label: 'Auto', value: 0 },
  { label: '100 KB/s', value: 100 * 1024 },
  { label: '500 KB/s', value: 500 * 1024 },
  { label: '1 MB/s', value: 1024 * 1024 },
  { label: '5 MB/s', value: 5 * 1024 * 1024 },
  { label: '20 MB/s', value: 20 * 1024 * 1024 },
] as const
