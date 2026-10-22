/**
 * Usage data plumbing.
 *
 * The live tail rides the one-second `usage:sample` event; everything longer is
 * already rolled up by the backend and simply refetched. Appending to a rolled
 * bucket client-side would double-count, so we never try.
 */

import { useEffect, useState, useSyncExternalStore } from 'react'

import { api, onUsageSample } from '@/lib/ipc'
import {
  EMPTY_RANKED,
  EMPTY_SUMMARY,
  type Ranked,
  type SpanTimeline,
  type UsageRange,
  type UsageSample,
  type UsageSpan,
  type UsageSummary,
} from '@/lib/types'

import { periodBounds, periodKey, type Period } from './periods'

/** Matches `SECONDS_KEPT` in the backend's history tiers. */
const LIVE_POINTS = 300
/** Rolled-up ranges only move once a minute; a slower poll is plenty. */
const REFETCH_MS = 15_000
const SUMMARY_REFETCH_MS = 10_000

const NO_SAMPLES: UsageSample[] = []

export function useTimeline(range: UsageRange, paused: boolean) {
  // Tagged with the range they belong to: until the new range's series lands,
  // the old one must not be drawn at the new range's bucket size.
  const [loaded, setLoaded] = useState<{ range: UsageRange; points: UsageSample[] }>({
    range,
    points: NO_SAMPLES,
  })

  const live = range === 'fiveMinutes'

  useEffect(() => {
    let cancelled = false

    const load = async () => {
      try {
        const series = await api.usageTimeline(range)
        if (!cancelled) setLoaded({ range, points: series })
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
      setLoaded((previous) => {
        if (previous.range !== 'fiveMinutes') return previous
        const next = [...previous.points, sample]
        return {
          range: previous.range,
          points: next.length > LIVE_POINTS ? next.slice(next.length - LIVE_POINTS) : next,
        }
      })
    })
    return () => {
      void subscription.then((unlisten) => unlisten())
    }
  }, [live, paused])

  return loaded.range === range ? loaded.points : NO_SAMPLES
}

/** A closed period in the past cannot change; only one reaching today does. */
function reachesToday(toTs: number): boolean {
  return toTs >= Date.now() / 1000 - 86_400
}

export function useSummary(period: Period) {
  const [summary, setSummary] = useState<UsageSummary>(EMPTY_SUMMARY)

  // Keyed on the period's identity rather than the object, so a re-render that
  // rebuilds an equal period does not refetch.
  const key = periodKey(period)
  const { fromTs, toTs } = period

  useEffect(() => {
    let cancelled = false

    const load = async () => {
      try {
        const next = await api.usageSummaryBetween(fromTs, toTs)
        if (!cancelled) setSummary(next)
      } catch {
        // Same reasoning as the timeline: stale numbers beat empty ones.
      }
    }

    void load()

    const timer = reachesToday(toTs)
      ? window.setInterval(load, SUMMARY_REFETCH_MS)
      : undefined
    return () => {
      cancelled = true
      if (timer) window.clearInterval(timer)
    }
  }, [key, fromTs, toTs])

  return summary
}

const EMPTY_SPAN_TIMELINE: SpanTimeline = { bucketSecs: 3600, samples: [] }

/**
 * The traffic shape of a whole period, for the strip under the Usage columns.
 * Polled only while the period reaches today; a past one is fetched once.
 */
export function usePeriodTimeline(period: Period) {
  const [loaded, setLoaded] = useState<{ key: string; timeline: SpanTimeline }>({
    key: '',
    timeline: EMPTY_SPAN_TIMELINE,
  })

  const key = periodKey(period)
  const { fromTs, toTs } = period

  useEffect(() => {
    let cancelled = false
    const [from, to] = periodBounds({ fromTs, toTs })

    const load = async () => {
      try {
        const timeline = await api.usageTimelineBetween(from, to)
        if (!cancelled) setLoaded({ key, timeline })
      } catch {
        // An empty strip is the honest answer to a failed read.
        if (!cancelled) setLoaded({ key, timeline: EMPTY_SPAN_TIMELINE })
      }
    }

    void load()
    const timer = reachesToday(toTs)
      ? window.setInterval(load, REFETCH_MS)
      : undefined
    return () => {
      cancelled = true
      if (timer) window.clearInterval(timer)
    }
  }, [key, fromTs, toTs])

  // A different period's strip is never shown under this one's columns.
  return loaded.key === key ? loaded.timeline : EMPTY_SPAN_TIMELINE
}

/** The stored period, for building the month list and bounding the pickers. */
export function useUsageSpan(): UsageSpan | null {
  const [span, setSpan] = useState<UsageSpan | null>(null)

  useEffect(() => {
    let cancelled = false
    void api
      .usageSpan()
      .then((next) => {
        if (!cancelled) setSpan(next)
      })
      .catch(() => {
        // No span means no month entries - the rolling presets still work.
      })
    return () => {
      cancelled = true
    }
  }, [])

  return span
}

/**
 * Windows' own per-app figures for a resolved span - the Apps column's source
 * when the network monitor has nothing for the period.
 *
 * Kept apart from `useSummary` because it is a different animal: Windows takes
 * several seconds to answer, so this loads once per period, is never polled,
 * and is skipped entirely (`enabled: false`) whenever the monitor's figures
 * are there to be shown instead.
 */
export function useWindowsApps(fromTs: number, toTs: number, enabled: boolean) {
  // The loaded span is stored alongside its rows, so "still loading" is derived
  // from a mismatch rather than from a flag set inside the effect.
  const [loaded, setLoaded] = useState<{ key: string; ranked: Ranked }>({
    key: '',
    ranked: EMPTY_RANKED,
  })

  const key = `${fromTs}:${toTs}`
  const wanted = enabled && fromTs > 0 && toTs > 0

  useEffect(() => {
    if (!wanted) return

    let cancelled = false
    void api
      .usageDeviceApps(fromTs, toTs)
      .then((ranked) => {
        if (!cancelled) setLoaded({ key, ranked })
      })
      .catch(() => {
        // Falls back to the relay-attributed column, which is always there.
        if (!cancelled) setLoaded({ key, ranked: EMPTY_RANKED })
      })

    return () => {
      cancelled = true
    }
  }, [key, fromTs, toTs, wanted])

  const ready = loaded.key === key
  return {
    ranked: ready ? loaded.ranked : EMPTY_RANKED,
    loading: wanted && !ready,
  }
}

// ---------------------------------------------------------------------------
// App icons
// ---------------------------------------------------------------------------

/*
 * One cache for the whole app session. Icons never change while the app runs,
 * and the summary they decorate is re-polled every ten seconds - fetching them
 * with it would ship the same PNGs across the bridge over and over.
 *
 * The cache is an immutable snapshot, replaced whenever a batch lands. A new
 * identity is what lets memoised consumers (the React Compiler memoises every
 * component here) notice that there is something new to draw. `null` records
 * "asked, has none", so a missing icon is not re-requested.
 */
let iconSnapshot: ReadonlyMap<string, string | null> = new Map()
const inFlight = new Set<string>()
const listeners = new Set<() => void>()

function subscribeIcons(listener: () => void) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

function readIcons() {
  return iconSnapshot
}

function requestIcons(keys: string[]) {
  const missing = keys.filter((key) => !iconSnapshot.has(key) && !inFlight.has(key))
  if (missing.length === 0) return

  for (const key of missing) inFlight.add(key)
  void api
    .usageIcons(missing)
    .then((found) => {
      const next = new Map(iconSnapshot)
      for (const key of missing) next.set(key, found[key] ?? null)
      iconSnapshot = next
      for (const listener of listeners) listener()
    })
    .catch(() => {
      // Left uncached, so the next render asks again.
    })
    .finally(() => {
      for (const key of missing) inFlight.delete(key)
    })
}

/** Icon lookup for a set of app names, fetching whatever is not cached yet. */
export function useAppIcons(keys: readonly string[]) {
  const icons = useSyncExternalStore(subscribeIcons, readIcons)

  // App names cannot contain a newline, so it is a safe separator - and a
  // stable string means a re-polled summary with the same apps asks nothing.
  const signature = keys.join('\n')
  useEffect(() => {
    if (signature) requestIcons(signature.split('\n'))
  }, [signature])

  return (key: string) => icons.get(key) ?? undefined
}
