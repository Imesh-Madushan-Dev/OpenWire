/** Protection features: the switches that change how traffic is handled. */

import {
  Rocket01Icon,
  Route01Icon,
  Shield01Icon,
  Wifi01Icon,
} from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'

import { SettingGroup, SettingRow } from '@/components/setting-row'
import { staggerParent } from '@/lib/motion'
import { isActive } from '@/lib/types'
import { useVpn } from '@/features/vpn/vpn-provider'

import { ViewShell } from '@/components/view-shell'

export function ProtectionView() {
  const { settings, platform, connection, updateSettings } = useVpn()

  // Wait for the real values rather than rendering switches in a guessed state.
  if (!settings) return <ViewShell title="Features" />

  const live = isActive(connection.phase)
  const restartHint = live ? 'Reconnect to apply this change.' : undefined
  const proxyUnsupported = platform ? !platform.systemProxySupported : false

  return (
    <ViewShell
      title="Features"
      description="How Maizy handles your traffic while the tunnel is up."
    >
      <motion.div
        variants={staggerParent}
        initial="initial"
        animate="animate"
        className="flex flex-col gap-6"
      >
        <SettingGroup title="Leak protection">
          <SettingRow
            icon={Shield01Icon}
            title="Kill switch"
            description="Block traffic instead of letting it out unmasked if the server becomes unreachable."
            checked={settings.killSwitch}
            onChange={(next) => void updateSettings({ killSwitch: next })}
            hint={restartHint}
          />
          <SettingRow
            icon={Route01Icon}
            title="Bypass local network"
            description="Keep printers, routers and other devices on your network reachable directly."
            checked={settings.lanBypass}
            onChange={(next) => void updateSettings({ lanBypass: next })}
            hint={restartHint}
          />
        </SettingGroup>

        <SettingGroup title="Routing">
          <SettingRow
            icon={Wifi01Icon}
            title="Route all apps"
            description="Point Windows at the local proxy so every app goes through the tunnel."
            checked={settings.systemProxy && !proxyUnsupported}
            onChange={(next) => void updateSettings({ systemProxy: next })}
            disabled={proxyUnsupported}
            hint={
              proxyUnsupported
                ? `Automatic routing isn't available on ${platform?.platform ?? 'this platform'} yet - point apps at the proxy address shown on the dashboard.`
                : restartHint
            }
          />
        </SettingGroup>

        <SettingGroup title="Startup">
          <SettingRow
            icon={Rocket01Icon}
            title="Connect on launch"
            description="Reconnect to your last location as soon as Maizy starts."
            checked={settings.autoConnect}
            onChange={(next) => void updateSettings({ autoConnect: next })}
          />
        </SettingGroup>
      </motion.div>
    </ViewShell>
  )
}
