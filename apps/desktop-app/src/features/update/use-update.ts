/**
 * Whether a newer OpenWire is out, as the backend last found it.
 *
 * The check runs in Rust - the webview has no network access - shortly after
 * launch and twice a day while automatic checks are on. `check` asks again
 * now, whatever that setting says.
 */

import { useEffect, useState } from 'react'
import { openUrl } from '@tauri-apps/plugin-opener'

import { api, onUpdateStatus, VpnError } from '@/lib/ipc'
import type { UpdateStatus } from '@/lib/types'
import { useVpn } from '@/features/vpn/vpn-context'

// Must match the opener scope in src-tauri/capabilities/default.json.
const RELEASE_URL = 'https://github.com/Imesh-Madushan-Dev/OpenWire/releases/latest'

export function useUpdate() {
  const { notify } = useVpn()
  const [status, setStatus] = useState<UpdateStatus | null>(null)
  const [checking, setChecking] = useState(false)

  useEffect(() => {
    let cancelled = false
    void api
      .updateStatus()
      .then((next) => {
        if (!cancelled) setStatus(next)
      })
      .catch(() => {
        // Nothing to show until a check lands; not worth a notice.
      })

    const subscription = onUpdateStatus((next) => setStatus(next))
    return () => {
      cancelled = true
      void subscription.then((unlisten) => unlisten())
    }
  }, [])

  /** Always answers with a notice: a check the user asked for that ends
   *  silently reads as a button that did nothing. */
  const check = async () => {
    setChecking(true)
    try {
      const next = await api.checkUpdate()
      setStatus(next)
      notify({
        level: 'info',
        message: next.available
          ? `OpenWire ${next.latest} is available - get it from the OpenWire menu or Settings > About.`
          : `You have the latest version, ${next.current}.`,
      })
    } catch (error) {
      notify({
        level: 'error',
        message: error instanceof VpnError ? error.message : 'Could not check for updates.',
      })
    } finally {
      setChecking(false)
    }
  }

  const download = () =>
    void openUrl(RELEASE_URL).catch(() =>
      notify({ level: 'error', message: 'Could not open the browser.' }),
    )

  return {
    status,
    /** The newer version, while there is one. */
    available: status?.available ? status.latest : null,
    checking,
    check,
    /** Opens the release page, where the installer is. */
    download,
  }
}
