/** The connection log's lines (`core/connlog.rs`), kept current by live events. */

import { useEffect, useState } from 'react'

import { api, onLog } from '@/lib/ipc'
import type { LogEntry } from '@/lib/types'

/** The backend keeps 500; the UI holds the same. */
const MAX_ENTRIES = 500

/** Fetched once, then kept current by live events; merged by id either way. */
export function useConnectionLog() {
  const [entries, setEntries] = useState<LogEntry[]>([])

  useEffect(() => {
    let cancelled = false
    const merge = (incoming: LogEntry[]) =>
      setEntries((current) => {
        const byId = new Map(current.map((entry) => [entry.id, entry]))
        for (const entry of incoming) byId.set(entry.id, entry)
        return [...byId.values()].sort((a, b) => a.id - b.id).slice(-MAX_ENTRIES)
      })

    // Subscribed before fetching, so a line logged in between is not lost.
    const subscription = onLog((entry) => merge([entry]))
    api
      .logs()
      .then((list) => {
        if (!cancelled) merge(list)
      })
      .catch(() => {
        // Live lines still arrive; the backlog is a nicety.
      })

    return () => {
      cancelled = true
      void subscription.then((unlisten) => unlisten())
    }
  }, [])

  const clear = async () => {
    await api.clearLogs()
    setEntries([])
  }

  return { entries, clear }
}
