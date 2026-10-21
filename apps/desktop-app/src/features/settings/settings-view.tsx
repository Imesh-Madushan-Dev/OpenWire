/** Settings: protection, routing, servers, app behaviour, diagnostics, and about. */

import { useEffect, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ChartHistogramIcon,
  ComputerIcon,
  Delete02Icon,
  GithubIcon,
  InformationCircleIcon,
  Rocket01Icon,
  Route01Icon,
  ServerIcon,
  Shield01Icon,
  Wifi01Icon,
} from '@hugeicons/core-free-icons'
import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart'
import { openUrl } from '@tauri-apps/plugin-opener'
import { motion } from 'motion/react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { SettingGroup, SettingRow } from '@/components/setting-row'
import { ViewShell } from '@/components/view-shell'
import { staggerChild, staggerParent } from '@/lib/motion'
import { isActive } from '@/lib/types'
import { ServerImportField } from '@/features/servers/server-import-field'
import { RestoreNetworkRow } from './restore-network-row'
import { useVpn } from '@/features/vpn/vpn-context'

// Must match the opener scope in src-tauri/capabilities/default.json.
const DEVELOPER_URL = 'https://github.com/Imesh-Madushan-Dev'

export function SettingsView() {
  const { settings, platform, servers, connection, updateSettings, removeServer, notify } =
    useVpn()

  // Wait for the real values rather than rendering switches in a guessed state.
  if (!settings) return <ViewShell title="Settings" />

  const live = isActive(connection.phase)
  const restartHint = live ? 'Reconnect to apply this change.' : undefined
  const proxyUnsupported = platform ? !platform.systemProxySupported : false
  const customServers = servers.filter((server) => server.tags.includes('custom'))

  return (
    <ViewShell title="Settings" description="How Maizy handles your traffic, and how the app behaves.">
      <motion.div
        variants={staggerParent}
        initial="initial"
        animate="animate"
        className="flex flex-col gap-6"
      >
        <SettingGroup title="Protection">
          <SettingRow
            icon={Shield01Icon}
            title="Kill switch"
            description="If the server drops, block everything rather than let traffic out unprotected. Your internet stops until the tunnel recovers - that is the trade."
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
          <SettingRow
            icon={ChartHistogramIcon}
            title="Measure usage while disconnected"
            description="Keep routing through Maizy when no VPN is connected, so the Network view can still name which apps and hosts used your bandwidth. Your traffic is not masked - it goes straight out, exactly as it would without Maizy."
            checked={settings.monitorWhenIdle && settings.systemProxy && !proxyUnsupported}
            onChange={(next) => void updateSettings({ monitorWhenIdle: next })}
            disabled={proxyUnsupported || !settings.systemProxy}
            hint={
              proxyUnsupported || !settings.systemProxy
                ? 'Needs "Route all apps" above, which is how traffic reaches Maizy to be counted.'
                : 'Only apps that follow the Windows proxy are counted; the total on the ring always covers everything.'
            }
          />
          {/* Keyed by the stored port: a change elsewhere remounts the field
              with the right value instead of syncing it through an effect. */}
          <PortRow key={settings.listenPort} port={settings.listenPort} />
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
                      disabled={live && connection.serverId === server.id}
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

        <SettingGroup title="App">
          <StartWithWindowsRow />
          <SettingRow
            icon={Rocket01Icon}
            title="Connect on launch"
            description="Reconnect to your last location as soon as Maizy starts."
            checked={settings.autoConnect}
            onChange={(next) => void updateSettings({ autoConnect: next })}
          />
          <SettingRow
            icon={ComputerIcon}
            title="Start in the tray"
            description="Launch without opening the window. Maizy keeps running in the background."
            checked={settings.launchMinimized}
            onChange={(next) => void updateSettings({ launchMinimized: next })}
          />
        </SettingGroup>

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
          <RestoreNetworkRow />
        </SettingGroup>

        <SettingGroup title="About">
          <SettingRow
            icon={GithubIcon}
            title="Developer"
            description="Imesh Madushan"
            control={
              <Button
                variant="outline"
                size="sm"
                className="shrink-0"
                onClick={() =>
                  void openUrl(DEVELOPER_URL).catch(() =>
                    notify({ level: 'error', message: 'Could not open the browser.' }),
                  )
                }
              >
                GitHub
              </Button>
            }
          />
        </SettingGroup>

        <motion.div variants={staggerChild} className="flex flex-col gap-4 px-1">
          <p className="flex items-start gap-2 text-xs leading-relaxed text-muted-foreground">
            <HugeiconsIcon
              icon={InformationCircleIcon}
              size={14}
              strokeWidth={1.8}
              className="mt-0.5 shrink-0"
            />
            Server credentials are stored in your local app config folder and are never sent
            anywhere. Closing the window keeps the tunnel running - quit from the tray to stop
            it and restore your network settings.
          </p>
          <p className="text-center text-xs text-muted-foreground">
            Built with ❤️ for Sri Lanka 🇱🇰
          </p>
        </motion.div>
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

/** Registers Maizy with the OS login items; the OS entry is the source of truth. */
function StartWithWindowsRow() {
  const [on, setOn] = useState(false)

  useEffect(() => {
    isEnabled().then(setOn).catch(() => {})
  }, [])

  const toggle = async (next: boolean) => {
    setOn(next)
    try {
      await (next ? enable() : disable())
    } catch {
      setOn(!next)
    }
  }

  return (
    <SettingRow
      icon={ComputerIcon}
      title="Start with Windows"
      description="Open Maizy automatically when you sign in."
      checked={on}
      onChange={(next) => void toggle(next)}
    />
  )
}
