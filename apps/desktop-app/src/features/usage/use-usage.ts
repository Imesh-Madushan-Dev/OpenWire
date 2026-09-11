/**
 * Usage data plumbing.
 *
 * The live tail rides the one-second `usage:sample` event; everything longer is
 * already rolled up by the backend and simply refetched. Appending to a rolled
 * bucket client-side would double-count, so we never try.
 */

import { useEffect, useState } from 'react'

import { api, onUsageSample } from '@/lib/ipc'
import {
  EMPTY_SUMMARY,
  type UsageRange,
  type UsageSample,
  type UsageSummary,
} from '@/lib/types'

/** Matches `SECONDS_KEPT` in the backend's history tiers. */
const LIVE_POINTS = 300
/** Rolled-up ranges only move once a minute; a slower poll is plenty. */
const REFETCH_MS = 15_000
const SUMMARY_REFETCH_MS = 10_000

export function useTimeline(range: UsageRange, paused: boolean) {
  const [points, setPoints] = useState<UsageSample[]>([])

  const live = range === 'fiveMinutes'

  useEffect(() => {
    let cancelled = false

    const load = async () => {
      try {
        const series = await api.usageTimeline(range)
        if (!cancelled) setPoints(series)
      } catch {
        // Keep the last good series rather than blanking the graph on a blip.
      }
    }

    void load()

    // Rolled-up ranges are polled; the live range is driven by events below.
    const timer = live ? undefined : window.setInterval(load, REFETCH_MS)
    return () => {
      cancelled = true
      if (timer) window.clearInterval(timer)
    }
  }, [range, live])

  useEffect(() => {
    // Not subscribing at all beats subscribing and discarding: pausing really
    // stops the stream instead of quietly draining it.
    if (!live || paused) return

    const subscription = onUsageSample((sample) => {
      setPoints((previous) => {
        const next = [...previous, sample]
        return next.length > LIVE_POINTS ? next.slice(next.length - LIVE_POINTS) : next
      })
    })
    return () => {
      void subscription.then((unlisten) => unlisten())
    }
  }, [live, paused])

  return points
}

export function useSummary(range: UsageRange) {
  const [summary, setSummary] = useState<UsageSummary>(EMPTY_SUMMARY)

  useEffect(() => {
    let cancelled = false

    const load = async () => {
      try {
        const next = await api.usageSummary(range)
        if (!cancelled) setSummary(next)
      } catch {
        // Same reasoning as the timeline: stale numbers beat empty ones.
      }
    }

    void load()
    const timer = window.setInterval(load, SUMMARY_REFETCH_MS)
    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [range])

  return summary
}
