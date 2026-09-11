/** Settings: local proxy, servers, and diagnostics. */

import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ComputerIcon,
  Delete02Icon,
  InformationCircleIcon,
  ServerIcon,
} from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { SettingGroup, SettingRow } from '@/components/setting-row'
import { ViewShell } from '@/components/view-shell'
import { staggerChild, staggerParent } from '@/lib/motion'
import { isActive } from '@/lib/types'
import { ServerImportField } from '@/features/servers/server-import-field'
import { useVpn } from '@/features/vpn/vpn-context'

export function SettingsView() {
  const { settings, platform, servers, connection, updateSettings, removeServer } = useVpn()

  if (!settings) return <ViewShell title="Settings" />

  const customServers = servers.filter((server) => server.tags.includes('custom'))

  return (
    <ViewShell title="Settings" description="Local proxy, your servers, and diagnostics.">
      <motion.div
        variants={staggerParent}
        initial="initial"
        animate="animate"
        className="flex flex-col gap-6"
      >
        <SettingGroup title="Local proxy">
          {/* Keyed by the stored port: a change elsewhere remounts the field
              with the right value instead of syncing it through an effect. */}
          <PortRow key={settings.listenPort} port={settings.listenPort} />
          <SettingRow
            icon={ComputerIcon}
            title="Start in the tray"
            description="Launch without opening the window. Maizy keeps running in the background."
            checked={settings.launchMinimized}
            onChange={(next) => void updateSettings({ launchMinimized: next })}
          />
        </SettingGroup>

        <section className="flex flex-col gap-2">
          <h2 className="px-1 text-xs font-semibold tracking-wide text-muted-foreground uppercase">
            Your servers
          </h2>
          <div className="panel flex flex-col gap-4 rounded-2xl p-4">
            <ServerImportField />

            {customServers.length > 0 && (
              <ul className="flex flex-col divide-y divide-white/6">
                {customServers.map((server) => (
                  <motion.li
                    key={server.id}
                    variants={staggerChild}
                    layout
                    className="flex items-center gap-3 py-2.5"
                  >
                    <HugeiconsIcon
                      icon={ServerIcon}
                      size={16}
                      strokeWidth={1.8}
                      className="shrink-0 text-muted-foreground"
                    />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-sm font-medium">{server.name}</span>
                      {/* Host only - the access key never leaves the backend. */}
                      <span
                        className="block truncate font-mono text-xs text-muted-foreground"
                        data-selectable
                      >
                        {server.host}
                      </span>
                    </span>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label={`Remove ${server.name}`}
                      disabled={isActive(connection.phase) && connection.serverId === server.id}
                      onClick={() => void removeServer(server.id)}
                    >
                      <HugeiconsIcon
                        icon={Delete02Icon}
                        strokeWidth={2}
                        className="text-destructive"
                      />
                    </Button>
                  </motion.li>
                ))}
              </ul>
            )}
          </div>
        </section>

        <SettingGroup title="Diagnostics">
          <motion.dl variants={staggerChild} className="flex flex-col gap-2.5 px-4 py-3.5">
            <Fact label="Proxy address" value={connection.listenAddr ?? 'Not running'} mono />
            <Fact
              label="System routing"
              value={
                platform?.systemProxySupported
                  ? connection.systemProxy
                    ? 'Active'
                    : 'Available, not engaged'
                  : 'Manual on this platform'
              }
            />
            <Fact label="Platform" value={platform?.platform ?? '--'} />
            <Fact label="Version" value={platform?.version ?? '--'} />
          </motion.dl>
        </SettingGroup>

        <motion.p
          variants={staggerChild}
          className="flex items-start gap-2 px-1 text-xs leading-relaxed text-muted-foreground"
        >
          <HugeiconsIcon
            icon={InformationCircleIcon}
            size={14}
            strokeWidth={1.8}
            className="mt-0.5 shrink-0"
          />
          Server credentials are stored in your local app config folder and are never sent
          anywhere. Closing the window keeps the tunnel running - quit from the tray to stop
          it and restore your network settings.
        </motion.p>
      </motion.div>
    </ViewShell>
  )
}

function Fact({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex items-baseline justify-between gap-4">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd
        className={`truncate text-xs ${mono ? 'numeric font-mono' : ''} text-foreground/85`}
        data-selectable
      >
        {value}
      </dd>
    </div>
  )
}

/**
 * The port is committed on blur or Enter rather than per keystroke - otherwise
 * every intermediate value would be validated and rejected.
 */
function PortRow({ port }: { port: number }) {
  const { updateSettings, notify } = useVpn()
  const [draft, setDraft] = useState(String(port))

  const commit = () => {
    const parsed = Number.parseInt(draft, 10)
    if (parsed === port) return

    if (!Number.isFinite(parsed) || parsed < 1024 || parsed > 65535) {
      setDraft(String(port))
      notify({ level: 'error', message: 'Choose a port between 1024 and 65535.' })
      return
    }
    void updateSettings({ listenPort: parsed })
  }

  return (
    <SettingRow
      icon={ComputerIcon}
      title="Proxy port"
      description="The loopback port apps connect to. Change it only if something else uses this port."
      control={
        <Input
          type="number"
          inputMode="numeric"
          min={1024}
          max={65535}
          value={draft}
          aria-label="Proxy port"
          onChange={(event) => setDraft(event.target.value)}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === 'Enter') event.currentTarget.blur()
          }}
          className="numeric h-9 w-24 shrink-0 text-center"
        />
      }
    />
  )
}
