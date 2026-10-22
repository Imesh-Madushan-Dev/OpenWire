/**
 * The network monitor service, as the UI sees it.
 *
 * The raw status is a set of independent facts from two sources (the service
 * manager and the pipe). `monitorState` folds them into the one thing a person
 * cares about, so Settings and the Usage view can never describe the same
 * service differently.
 */

import { useEffect, useState } from 'react'

import { api, onMonitorStatus, VpnError } from '@/lib/ipc'
import type { MonitorStatus } from '@/lib/types'
import { useVpn } from '@/features/vpn/vpn-context'

export type MonitorState =
  | 'loading'
  | 'unsupported'
  | 'off'
  /** Installed and running, still connecting or opening its trace. */
  | 'starting'
  | 'active'
  /** Running, but Windows would not give it a trace session. */
  | 'failing'
  /** Installed, but the service is not running. */
  | 'stopped'
  /** Installed by an older OpenWire; needs reinstalling. */
  | 'outdated'

export function monitorState(status: MonitorStatus | null): MonitorState {
  if (!status) return 'loading'
  if (!status.supported) return 'unsupported'
  if (!status.installed) return 'off'
  if (status.outdated) return 'outdated'
  if (!status.running) return 'stopped'
  if (status.connected && status.collecting) return 'active'
  if (status.error) return 'failing'
  return 'starting'
}

/** One line on where the monitor stands, for under its switch. */
export function monitorSummary(state: MonitorState, error: string | null): string {
  switch (state) {
    case 'loading':
    case 'off':
      return 'Off. Usage names apps for all traffic, but hosts and traffic types only for what passes through OpenWire.'
    case 'unsupported':
      return 'Detailed monitoring needs Windows.'
    case 'starting':
      return 'Starting…'
    case 'active':
      return 'Recording every app, host and traffic type on this device.'
    case 'failing':
      return error ?? 'Windows would not start network tracing.'
    case 'stopped':
      return 'Installed, but the service is not running. Repair restarts it.'
    case 'outdated':
      return 'Installed by an older version of OpenWire. Repair updates it.'
  }
}

export function useMonitor() {
  const { notify } = useVpn()
  const [status, setStatus] = useState<MonitorStatus | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    let cancelled = false
    void api
      .netmonStatus()
      .then((next) => {
        if (!cancelled) setStatus(next)
      })
      .catch(() => {
        // Leaves the switch in its loading state rather than guessing.
      })

    const subscription = onMonitorStatus((next) => setStatus(next))
    return () => {
      cancelled = true
      void subscription.then((unlisten) => unlisten())
    }
  }, [])

  const change = async (enable: boolean) => {
    setBusy(true)
    try {
      setStatus(await (enable ? api.netmonEnable() : api.netmonDisable()))
      notify({
        level: 'info',
        message: enable
          ? 'Detailed monitoring is on. New traffic shows up in Usage within seconds.'
          : 'Detailed monitoring is off. What it already recorded stays in your history.',
      })
    } catch (error) {
      notify({
        level: 'error',
        message:
          error instanceof VpnError ? error.message : 'Could not change detailed monitoring.',
      })
    } finally {
      setBusy(false)
    }
  }

  return {
    status,
    state: monitorState(status),
    busy,
    /** Installs, or reinstalls to repair or update. */
    enable: () => change(true),
    disable: () => change(false),
  }
}
