/**
 * Context type and accessor, kept apart from the provider component so the
 * provider file exports only components (and stays Fast Refresh friendly).
 */

import { createContext, use } from 'react'

import type { DeviceKind, DevicesSnapshot, ScanProgress } from '@/lib/types'

export interface DevicesContextValue {
  /** `null` until the first answer from the backend. */
  snapshot: DevicesSnapshot | null
  /** Set while a sweep runs - this view's or the background watcher's. */
  progress: ScanProgress | null
  scanning: boolean
  /** Devices that joined since the user last looked. */
  newCount: number

  refresh: () => Promise<void>
  scan: () => Promise<void>
  rename: (mac: string, name: string | null) => Promise<boolean>
  /** `null` returns to the automatic guess. */
  setKind: (mac: string, kind: DeviceKind | null) => Promise<boolean>
  /** `null` marks every device as seen. */
  acknowledge: (mac: string | null) => Promise<void>
  forget: (mac: string) => Promise<void>
}

export const DevicesContext = createContext<DevicesContextValue | null>(null)

export function useDevices(): DevicesContextValue {
  const context = use(DevicesContext)
  if (!context) throw new Error('useDevices must be used inside <DevicesProvider>')
  return context
}
