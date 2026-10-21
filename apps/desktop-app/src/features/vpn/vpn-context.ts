/**
 * Context type and accessor, kept apart from the provider component so the
 * provider file exports only components (and stays Fast Refresh friendly).
 */

import { createContext, use } from 'react'

import type {
  ConnectionState,
  ImportReport,
  Notice,
  PlatformInfo,
  ServerEdit,
  ServerSummary,
  Settings,
  SettingsPatch,
  StatsSnapshot,
} from '@/lib/types'

export interface VpnContextValue {
  connection: ConnectionState
  stats: StatsSnapshot
  servers: ServerSummary[]
  settings: Settings | null
  platform: PlatformInfo | null
  optimal: ServerSummary | null
  /** The node currently in use, or the one that would be used next. */
  targetServer: ServerSummary | null
  notice: Notice | null
  probing: boolean
  ready: boolean

  toggle: () => Promise<void>
  connect: (serverId?: string | null) => Promise<void>
  disconnect: () => Promise<void>
  /** `null` means automatic: the fastest server at connect time. */
  chooseServer: (id: string | null) => Promise<void>
  probeServers: () => Promise<void>
  importServer: (uri: string) => Promise<ServerSummary | null>
  /** Imports every link in `text`, or on the clipboard when `text` is omitted. */
  importLinks: (text?: string) => Promise<ImportReport | null>
  updateServer: (id: string, edit: ServerEdit) => Promise<boolean>
  removeServer: (id: string) => Promise<void>
  updateSettings: (patch: SettingsPatch) => Promise<void>
  notify: (notice: Notice) => void
  dismissNotice: () => void
}

export const VpnContext = createContext<VpnContextValue | null>(null)

export function useVpn(): VpnContextValue {
  const context = use(VpnContext)
  if (!context) throw new Error('useVpn must be used inside <VpnProvider>')
  return context
}
