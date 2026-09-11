/**
 * Single source of truth for the UI.
 *
 * The backend is authoritative: this provider never guesses what the tunnel is
 * doing. It seeds from commands on mount and then follows `vpn:state` /
 * `vpn:stats` events, so the tray and the window can never disagree.
 */

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'

import { api, onConnectionState, onNotice, onStats, VpnError } from '@/lib/ipc'
import {
  INITIAL_CONNECTION,
  INITIAL_STATS,
  isActive,
  isBusy,
  type ConnectionState,
  type Notice,
  type PlatformInfo,
  type ServerSummary,
  type Settings,
  type SettingsPatch,
  type StatsSnapshot,
} from '@/lib/types'

import { VpnContext, type VpnContextValue } from './vpn-context'

export function VpnProvider({ children }: { children: ReactNode }) {
  const [connection, setConnection] = useState<ConnectionState>(INITIAL_CONNECTION)
  const [stats, setStats] = useState<StatsSnapshot>(INITIAL_STATS)
  const [servers, setServers] = useState<ServerSummary[]>([])
  const [settings, setSettings] = useState<Settings | null>(null)
  const [platform, setPlatform] = useState<PlatformInfo | null>(null)
  const [optimal, setOptimal] = useState<ServerSummary | null>(null)
  const [notice, setNotice] = useState<Notice | null>(null)
  const [probing, setProbing] = useState(false)
  const [ready, setReady] = useState(false)

  /** Guards against overlapping connect/disconnect from the UI and the tray. */
  const inFlight = useRef(false)

  /**
   * Single funnel for connection updates. Zeroing the readout here - rather
   * than in an effect watching the phase - avoids a cascading re-render and
   * keeps the two pieces of state in step within one commit.
   */
  const applyConnection = useCallback((next: ConnectionState) => {
    setConnection(next)
    if (next.phase === 'disconnected') setStats(INITIAL_STATS)
  }, [])

  const notify = useCallback((next: Notice) => setNotice(next), [])
  const dismissNotice = useCallback(() => setNotice(null), [])

  /** Turns any rejection into a friendly notice; never rethrows into render. */
  const surface = useCallback((error: unknown) => {
    const message =
      error instanceof VpnError
        ? error.message
        : 'Something went wrong. Please try again.'
    if (!(error instanceof VpnError)) console.error(error)
    setNotice({ level: 'error', message })
  }, [])

  // -- initial load + event subscriptions ----------------------------------

  useEffect(() => {
    let cancelled = false

    const load = async () => {
      try {
        const [status, serverList, storedSettings, platformInfo, best] =
          await Promise.all([
            api.status(),
            api.servers(),
            api.settings(),
            api.platform(),
            api.optimalServer(),
          ])
        if (cancelled) return
        applyConnection(status)
        setServers(serverList)
        setSettings(storedSettings)
        setPlatform(platformInfo)
        setOptimal(best)
      } catch (error) {
        if (!cancelled) surface(error)
      } finally {
        if (!cancelled) setReady(true)
      }
    }

    void load()
    return () => {
      cancelled = true
    }
  }, [applyConnection, surface])

  useEffect(() => {
    // `listen` resolves to an unlisten fn; collect them so StrictMode's double
    // mount cannot leave a duplicate subscription behind.
    const pending = [
      onConnectionState(applyConnection),
      onStats(setStats),
      onNotice(setNotice),
    ]

    return () => {
      for (const subscription of pending) {
        void subscription.then((unlisten) => unlisten())
      }
    }
  }, [applyConnection])

  // -- actions --------------------------------------------------------------

  const refreshCatalog = useCallback(async () => {
    const [serverList, best] = await Promise.all([api.servers(), api.optimalServer()])
    setServers(serverList)
    setOptimal(best)
  }, [])

  const connect = useCallback(
    async (serverId?: string | null) => {
      if (inFlight.current) return
      inFlight.current = true
      try {
        applyConnection(await api.connect(serverId ?? null))
      } catch (error) {
        surface(error)
      } finally {
        inFlight.current = false
      }
    },
    [applyConnection, surface],
  )

  const disconnect = useCallback(async () => {
    if (inFlight.current) return
    inFlight.current = true
    try {
      applyConnection(await api.disconnect())
    } catch (error) {
      surface(error)
    } finally {
      inFlight.current = false
    }
  }, [applyConnection, surface])

  const toggle = useCallback(async () => {
    if (isBusy(connection.phase)) return
    await (isActive(connection.phase) ? disconnect() : connect())
  }, [connection.phase, connect, disconnect])

  /** Picks a server, remembers it, and moves a live session over to it. */
  const chooseServer = useCallback(
    async (id: string) => {
      try {
        setSettings(await api.updateSettings({ preferredServer: id }))
        if (isActive(connection.phase)) {
          await disconnect()
          await connect(id)
        }
      } catch (error) {
        surface(error)
      }
    },
    [connection.phase, connect, disconnect, surface],
  )

  const probeServers = useCallback(async () => {
    setProbing(true)
    try {
      setServers(await api.probeServers())
      setOptimal(await api.optimalServer())
    } catch (error) {
      surface(error)
    } finally {
      setProbing(false)
    }
  }, [surface])

  const importServer = useCallback(
    async (uri: string) => {
      try {
        const imported = await api.importServer(uri)
        await refreshCatalog()
        setNotice({ level: 'info', message: `Added ${imported.name}.` })
        return imported
      } catch (error) {
        surface(error)
        return null
      }
    },
    [refreshCatalog, surface],
  )

  const removeServer = useCallback(
    async (id: string) => {
      try {
        await api.removeServer(id)
        await refreshCatalog()
        setSettings(await api.settings())
      } catch (error) {
        surface(error)
      }
    },
    [refreshCatalog, surface],
  )

  const updateSettings = useCallback(
    async (patch: SettingsPatch) => {
      try {
        setSettings(await api.updateSettings(patch))
      } catch (error) {
        surface(error)
        // Re-read so a rejected write cannot leave a toggle visually flipped.
        try {
          setSettings(await api.settings())
        } catch {
          /* the notice above already told the user */
        }
      }
    },
    [surface],
  )

  // The node the orb is about to use: live session first, then preference,
  // then whatever the backend considers optimal.
  const targetServer = useMemo(() => {
    const byId = (id: string | null | undefined) =>
      id ? (servers.find((server) => server.id === id) ?? null) : null
    return byId(connection.serverId) ?? byId(settings?.preferredServer) ?? optimal
  }, [connection.serverId, servers, settings?.preferredServer, optimal])

  const value = useMemo<VpnContextValue>(
    () => ({
      connection,
      stats,
      servers,
      settings,
      platform,
      optimal,
      targetServer,
      notice,
      probing,
      ready,
      toggle,
      connect,
      disconnect,
      chooseServer,
      probeServers,
      importServer,
      removeServer,
      updateSettings,
      notify,
      dismissNotice,
    }),
    [
      connection,
      stats,
      servers,
      settings,
      platform,
      optimal,
      targetServer,
      notice,
      probing,
      ready,
      toggle,
      connect,
      disconnect,
      chooseServer,
      probeServers,
      importServer,
      removeServer,
      updateSettings,
      notify,
      dismissNotice,
    ],
  )

  return <VpnContext value={value}>{children}</VpnContext>
}
