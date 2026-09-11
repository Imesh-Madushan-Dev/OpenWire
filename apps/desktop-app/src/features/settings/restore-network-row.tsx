/**
 * Manual repair for a stranded system proxy.
 *
 * The same recovery runs automatically at launch, but someone whose browsing
 * just stopped needs a button they can press now rather than a relaunch they
 * have no reason to guess at.
 */

import { useState } from 'react'
import { RefreshIcon } from '@hugeicons/core-free-icons'

import { Button } from '@/components/ui/button'
import { SettingRow } from '@/components/setting-row'
import { api, VpnError } from '@/lib/ipc'
import { isActive } from '@/lib/types'
import { useVpn } from '@/features/vpn/vpn-context'

export function RestoreNetworkRow() {
  const { connection, notify } = useVpn()
  const [working, setWorking] = useState(false)

  // Repairing while the tunnel is up would cut the proxy out from under it.
  const connected = isActive(connection.phase)

  const restore = async () => {
    setWorking(true)
    try {
      notify({ level: 'info', message: await api.restoreNetwork() })
    } catch (error) {
      notify({
        level: 'error',
        message:
          error instanceof VpnError
            ? error.message
            : 'Could not restore your network settings.',
      })
    } finally {
      setWorking(false)
    }
  }

  return (
    <SettingRow
      icon={RefreshIcon}
      title="Restore network settings"
      description="Undo a leftover proxy if browsing stopped working after a crash or a forced quit."
      control={
        <Button
          variant="outline"
          size="sm"
          onClick={() => void restore()}
          disabled={working || connected}
          className="shrink-0"
        >
          {working ? 'Restoring' : 'Restore'}
        </Button>
      }
      hint={connected ? 'Disconnect first - the proxy is in use right now.' : undefined}
    />
  )
}
