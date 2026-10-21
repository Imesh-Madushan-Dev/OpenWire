/**
 * Live device list for the whole app: the Devices view reads it, and so does
 * the title bar, whose tab shows how many devices are new. Both stay in step
 * through the backend's events, whoever started the sweep.
 */

import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'

import { api, onDevices, onDevicesProgress } from '@/lib/ipc'
import type { DeviceKind, DevicesSnapshot, ScanProgress } from '@/lib/types'
import { useVpn } from '@/features/vpn/vpn-context'

import { DevicesContext, type DevicesContextValue } from './devices-context'

export function DevicesProvider({ children }: { children: ReactNode }) {
  const { notify } = useVpn()
  const [snapshot, setSnapshot] = useState<DevicesSnapshot | null>(null)
  const [progress, setProgress] = useState<ScanProgress | null>(null)

  useEffect(() => {
    let cancelled = false
    void api
      .devices()
      .then((next) => {
        if (!cancelled) setSnapshot(next)
      })
      .catch(() => {
        // Leaves the view in its loading state rather than guessing.
      })

    const subscriptions = [
      onDevices((next) => {
        setSnapshot(next)
        setProgress(null)
      }),
      onDevicesProgress(setProgress),
    ]
    return () => {
      cancelled = true
      for (const subscription of subscriptions) void subscription.then((stop) => stop())
    }
  }, [])

  const fail = useCallback(
    (error: unknown) =>
      notify({
        level: 'error',
        message: error instanceof Error ? error.message : 'Something went wrong. Please try again.',
      }),
    [notify],
  )

  /** Runs an edit; resolves to whether it stuck. */
  const apply = useCallback(
    async (action: () => Promise<DevicesSnapshot>) => {
      try {
        setSnapshot(await action())
        return true
      } catch (error) {
        fail(error)
        return false
      }
    },
    [fail],
  )

  const refresh = useCallback(async () => {
    try {
      setSnapshot(await api.devices())
    } catch {
      // A failed refresh keeps the last good list on screen.
    }
  }, [])

  const scan = useCallback(async () => {
    // Shown at once; the backend's first progress event follows within a moment.
    setProgress({ done: 0, total: 0 })
    try {
      setSnapshot(await api.scanDevices())
    } catch (error) {
      fail(error)
    } finally {
      setProgress(null)
    }
  }, [fail])

  // Stable across list updates: the Devices view marks newcomers as seen when
  // it unmounts, and an action that changed identity on every sweep would set
  // that off while the user was still looking.
  const rename = useCallback(
    (mac: string, name: string | null) => apply(() => api.renameDevice(mac, name)),
    [apply],
  )
  const setKind = useCallback(
    (mac: string, kind: DeviceKind | null) => apply(() => api.setDeviceKind(mac, kind)),
    [apply],
  )
  const acknowledge = useCallback(
    async (mac: string | null) => {
      await apply(() => api.acknowledgeDevices(mac))
    },
    [apply],
  )
  const forget = useCallback(
    async (mac: string) => {
      await apply(() => api.forgetDevice(mac))
    },
    [apply],
  )

  const value = useMemo<DevicesContextValue>(
    () => ({
      snapshot,
      progress,
      scanning: progress !== null || Boolean(snapshot?.scanning),
      newCount: snapshot?.devices.filter((device) => device.isNew).length ?? 0,
      refresh,
      scan,
      rename,
      setKind,
      acknowledge,
      forget,
    }),
    [snapshot, progress, refresh, scan, rename, setKind, acknowledge, forget],
  )

  return <DevicesContext value={value}>{children}</DevicesContext>
}
