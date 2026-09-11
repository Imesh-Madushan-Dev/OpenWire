/**
 * Contextual next step. Shows the single most useful action for the current
 * setup rather than a static advertisement - and disappears once there is
 * nothing worth suggesting.
 */

import { HugeiconsIcon } from '@hugeicons/react'
import { PlusSignIcon, Shield01Icon, Wifi01Icon } from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'

import { Button } from '@/components/ui/button'
import { staggerChild } from '@/lib/motion'
import { isActive } from '@/lib/types'
import { useVpn } from '@/features/vpn/vpn-provider'

interface ActionCardProps {
  onAddServer: () => void
  onOpenFeatures: () => void
}

export function ActionCard({ onAddServer, onOpenFeatures }: ActionCardProps) {
  const { servers, settings, connection, platform } = useVpn()

  const suggestion = (() => {
    if (!servers.some((server) => server.configured && server.protocol !== 'direct')) {
      return {
        icon: PlusSignIcon,
        title: 'Add your server',
        body: 'Paste a vless:// link to start masking traffic',
        action: 'Add',
        onAction: onAddServer,
      }
    }

    if (settings && !settings.killSwitch) {
      return {
        icon: Shield01Icon,
        title: 'Turn on the kill switch',
        body: 'Blocks traffic instead of leaking if the tunnel drops',
        action: 'Enable',
        onAction: onOpenFeatures,
      }
    }

    // Connected without the OS proxy: the address is the only way in, so
    // surface it rather than leaving the user to guess.
    if (isActive(connection.phase) && !connection.systemProxy && connection.listenAddr) {
      return {
        icon: Wifi01Icon,
        title: `Proxy at ${connection.listenAddr}`,
        body: platform?.systemProxySupported
          ? 'System proxy is off - point apps here manually'
          : 'Point your apps at this address to use the tunnel',
        action: 'Settings',
        onAction: onOpenFeatures,
      }
    }

    return null
  })()

  if (!suggestion) return null

  return (
    <motion.div
      variants={staggerChild}
      layout
      className="panel flex items-center gap-3 rounded-2xl px-4 py-3"
    >
      <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-primary/10">
        <HugeiconsIcon
          icon={suggestion.icon}
          size={18}
          strokeWidth={1.8}
          className="text-primary"
        />
      </span>

      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm font-semibold">{suggestion.title}</span>
        <span className="block truncate text-xs text-muted-foreground">{suggestion.body}</span>
      </span>

      <Button size="sm" onClick={suggestion.onAction} className="shrink-0">
        {suggestion.action}
      </Button>
    </motion.div>
  )
}
