/**
 * System-wide routing ("all apps"), and the tunnel service it runs through.
 *
 * Shared by Settings and the VPN page's quick toggle, so both turn it on the
 * same way: if the service is missing or broken, set it up first - Windows
 * asks for admin approval once - and only then save the setting. Saving it
 * alone would leave the next connect to fail with nothing done about it.
 */

import { useEffect, useState } from 'react'

import { api, VpnError } from '@/lib/ipc'
import type { TunServiceStatus } from '@/lib/types'
import { useVpn } from './vpn-context'

/** Not installed, not running, or refusing this app: set up again. */
function needsSetup(status: TunServiceStatus | null): boolean {
  return !status || !status.installed || !status.running || status.problem !== null
}

export function useTunMode() {
  const { settings, updateSettings, notify } = useVpn()
  const [service, setService] = useState<TunServiceStatus | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    let cancelled = false
    void api
      .tunStatus()
      .then((next) => {
        if (!cancelled) setService(next)
      })
      .catch(() => {
        // Leaves the row in its loading state rather than guessing.
      })
    return () => {
      cancelled = true
    }
  }, [])

  const fail = (error: unknown, fallback: string) =>
    notify({ level: 'error', message: error instanceof VpnError ? error.message : fallback })

  /** Installs, repairs or updates the service. True once it is usable. */
  const setUp = async (): Promise<boolean> => {
    setBusy(true)
    try {
      const next = await api.tunInstall()
      setService(next)
      if (next.problem) {
        notify({ level: 'error', message: next.problem })
        return false
      }
      return true
    } catch (error) {
      fail(error, 'Could not set up the tunnel service.')
      return false
    } finally {
      setBusy(false)
    }
  }

  const setEnabled = async (on: boolean) => {
    if (!on) {
      await updateSettings({ tunMode: false })
      return
    }
    // Asked afresh rather than trusting what loaded with the page: an
    // install prompt for a service that is already fine would be noise.
    let current = service
    try {
      current = await api.tunStatus()
      setService(current)
    } catch {
      // Fall through to setting it up, which reports its own failure.
    }
    if (current && !current.supported) return
    if (needsSetup(current) && !(await setUp())) return
    await updateSettings({ tunMode: true })
  }

  const enabled = settings?.tunMode ?? false
  return {
    service,
    busy,
    enabled,
    supported: service?.supported ?? true,
    /** On, but the service it needs is not usable. */
    needsRepair: enabled && service !== null && needsSetup(service),
    setEnabled,
    repair: setUp,
  }
}
