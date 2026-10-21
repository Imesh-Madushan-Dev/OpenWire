/**
 * Settings: a section sidebar beside the active section's controls - protection,
 * routing, servers, app behaviour, appearance, diagnostics, and about.
 */

import { useEffect, useRef, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ChartHistogramIcon,
  ComputerIcon,
  Delete02Icon,
  GithubIcon,
  InformationCircleIcon,
  Radar01Icon,
  Rocket01Icon,
  Route01Icon,
  ServerIcon,
  Shield01Icon,
  Wifi01Icon,
} from '@hugeicons/core-free-icons'
import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart'
import { openUrl } from '@tauri-apps/plugin-opener'
import { AnimatePresence, motion } from 'motion/react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Switch } from '@/components/ui/switch'
import { SettingGroup, SettingRow } from '@/components/setting-row'
import { staggerChild, staggerParent, transition } from '@/lib/motion'
import { isActive, type Settings } from '@/lib/types'
import { ServerImportField } from '@/features/servers/server-import-field'
import { monitorSummary, useMonitor } from '@/features/usage/use-monitor'
import { useVpn } from '@/features/vpn/vpn-context'
import { RestoreNetworkRow } from './restore-network-row'
import { SECTIONS, type SectionId } from './sections'
import { ChartPalettePicker } from './chart-palette-picker'
import { SettingsNav } from './settings-nav'
import { ThemePicker } from './theme-picker'

// Must match the opener scope in src-tauri/capabilities/default.json.
const DEVELOPER_URL = 'https://github.com/Imesh-Madushan-Dev'

const RESTART_HINT = 'Reconnect to apply this change.'

export function SettingsView() {
  const { settings } = useVpn()
  // Protection first: it is where the dashboard's feature link lands.
  const [active, setActive] = useState<SectionId>('protection')
  const scrollerRef = useRef<HTMLDivElement>(null)

  const section = SECTIONS.find((candidate) => candidate.id === active) ?? SECTIONS[0]

  const select = (next: SectionId) => {
    if (next === active) return
    setActive(next)
    scrollerRef.current?.scrollTo({ top: 0 })
  }

  return (
    <div ref={scrollerRef} className="h-full overflow-y-auto">
      <div className="mx-auto flex w-full max-w-208 gap-8 px-8 pt-4 pb-10">
        <SettingsNav active={active} onChange={select} />

        <div
          id="settings-panel"
          role="tabpanel"
          aria-labelledby={`settings-tab-${active}`}
          className="min-w-0 flex-1"
        >
          {/* Wait for the real values rather than rendering switches in a guessed state. */}
          {settings && (
            <AnimatePresence mode="wait">
              <motion.div
                key={active}
                variants={staggerParent}
                initial="initial"
                animate="animate"
                exit={{ opacity: 0, transition: transition.fast }}
                className="flex flex-col gap-6"
              >
                <motion.header variants={staggerChild} className="flex flex-col gap-1 pt-0.5">
                  <h2 className="font-heading text-lg font-semibold">{section.label}</h2>
                  <p className="text-sm leading-relaxed text-muted-foreground">
                    {section.description}
                  </p>
                </motion.header>

                <SectionBody id={active} settings={settings} />
              </motion.div>
            </AnimatePresence>
          )}
        </div>
      </div>
    </div>
  )
}

function SectionBody({ id, settings }: { id: SectionId; settings: Settings }) {
  switch (id) {
    case 'protection':
      return <ProtectionSection settings={settings} />
    case 'routing':
      return <RoutingSection settings={settings} />
    case 'monitoring':
      return <MonitoringSection settings={settings} />
    case 'servers':
      return <ServersSection />
    case 'general':
      return <GeneralSection settings={settings} />
    case 'appearance':
      return (
        <>
          <ThemePicker />
          <ChartPalettePicker />
        </>
      )
    case 'diagnostics':
      return <DiagnosticsSection />
    case 'about':
      return <AboutSection />
  }
}

function ProtectionSection({ settings }: { settings: Settings }) {
  const { connection, updateSettings } = useVpn()
  const restartHint = isActive(connection.phase) ? RESTART_HINT : undefined

  return (
    <SettingGroup>
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
  )
}

function RoutingSection({ settings }: { settings: Settings }) {
  const { platform, connection, updateSettings } = useVpn()
  const restartHint = isActive(connection.phase) ? RESTART_HINT : undefined
  const proxyUnsupported = platform ? !platform.systemProxySupported : false

  return (
    <SettingGroup>
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
      {/* Keyed by the stored port: a change elsewhere remounts the field
          with the right value instead of syncing it through an effect. */}
      <PortRow key={settings.listenPort} port={settings.listenPort} />
    </SettingGroup>
  )
}

function MonitoringSection({ settings }: { settings: Settings }) {
  const { platform, updateSettings } = useVpn()
  const monitor = useMonitor()
  const { state, status, busy } = monitor
  const proxyUnsupported = platform ? !platform.systemProxySupported : false
  const needsRepair = state === 'failing' || state === 'stopped' || state === 'outdated'

  return (
    <>
      <SettingGroup>
        <SettingRow
          icon={Radar01Icon}
          title="Detailed traffic monitoring"
          description="Count every app, host and traffic type on this device, not just what passes through Maizy. Installs a small background service; Windows asks for admin approval to turn it on or off."
          disabled={state === 'unsupported'}
          control={
            <div className="flex shrink-0 items-center gap-2">
              {needsRepair && (
                <Button
                  variant="outline"
                  size="sm"
                  disabled={busy}
                  onClick={() => void monitor.enable()}
                >
                  {state === 'outdated' ? 'Update' : 'Repair'}
                </Button>
              )}
              <Switch
                aria-label="Detailed traffic monitoring"
                checked={status?.installed ?? false}
                disabled={busy || state === 'loading' || state === 'unsupported'}
                onCheckedChange={(next) => void (next ? monitor.enable() : monitor.disable())}
              />
            </div>
          }
          hint={
            busy
              ? 'Waiting for Windows to approve the change…'
              : monitorSummary(state, status?.error ?? null)
          }
          hintTone={needsRepair ? 'warning' : 'muted'}
        />
        <SettingRow
          icon={ChartHistogramIcon}
          title="Measure usage while disconnected"
          description="Keep routing through Maizy when no VPN is connected, so it can name the hosts proxy-aware apps reach. Your traffic is not masked - it goes straight out, exactly as it would without Maizy."
          checked={settings.monitorWhenIdle && settings.systemProxy && !proxyUnsupported}
          onChange={(next) => void updateSettings({ monitorWhenIdle: next })}
          disabled={proxyUnsupported || !settings.systemProxy}
          hint={
            proxyUnsupported || !settings.systemProxy
              ? 'Needs "Route all apps" in Routing, which is how traffic reaches Maizy to be counted.'
              : state === 'active'
                ? 'Detailed monitoring already covers every app, so this is optional.'
                : 'Only apps that follow the Windows proxy are counted; the total on the ring always covers everything.'
          }
          hintTone={proxyUnsupported || !settings.systemProxy ? 'warning' : 'muted'}
        />
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
        Detailed monitoring sees which app reached which host and how much it moved - never the
        contents of your traffic - and nothing it records leaves this device. Turning it off
        removes the service; what it already recorded stays in your history until it ages out.
      </motion.p>
    </>
  )
}

function ServersSection() {
  const { servers, connection, removeServer } = useVpn()
  const live = isActive(connection.phase)
  const customServers = servers.filter((server) => server.tags.includes('custom'))

  return (
    <motion.div variants={staggerChild} className="panel flex flex-col gap-4 rounded-2xl p-4">
      <ServerImportField />

      {customServers.length > 0 && (
        <ul className="flex flex-col divide-y divide-tint/6">
          {customServers.map((server) => (
            <motion.li key={server.id} layout className="flex items-center gap-3 py-2.5">
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
                <HugeiconsIcon icon={Delete02Icon} strokeWidth={2} className="text-destructive" />
              </Button>
            </motion.li>
          ))}
        </ul>
      )}
    </motion.div>
  )
}

function GeneralSection({ settings }: { settings: Settings }) {
  const { updateSettings } = useVpn()

  return (
    <SettingGroup>
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
  )
}

function DiagnosticsSection() {
  const { platform, connection } = useVpn()

  return (
    <SettingGroup>
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
  )
}

function AboutSection() {
  const { notify } = useVpn()

  return (
    <>
      <SettingGroup>
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
          anywhere. Closing the window keeps the tunnel running - quit from the tray to stop it
          and restore your network settings.
        </p>
        {/* Required by the CC BY 4.0 licence of the bundled country database. */}
        <p className="text-xs leading-relaxed text-muted-foreground">
          Host locations use IP geolocation by DB-IP (db-ip.com), licensed under CC BY 4.0. Lookups
          run on this device; no address is sent anywhere to be located.
        </p>
        <p className="text-center text-xs text-muted-foreground">
          Built with ❤️ for Sri Lanka 🇱🇰
        </p>
      </motion.div>
    </>
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
