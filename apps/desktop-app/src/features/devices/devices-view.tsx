/**
 * Devices: everything else on this network - who it is, who made it, whether
 * it is here now, and which ones are new.
 */

import { useEffect, useMemo, useRef, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Radar01Icon,
  Search01Icon,
  Tick02Icon,
  WifiDisconnected02Icon,
} from '@hugeicons/core-free-icons'
import { AnimatePresence, motion } from 'motion/react'
import { cn } from 'cn'

import { Segmented } from '@/components/segmented'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { formatAgo } from '@/lib/format'
import { staggerParent, transition } from '@/lib/motion'
import type { Device } from '@/lib/types'
import { useVpn } from '@/features/vpn/vpn-context'

import { useDevices } from './devices-context'
import { DeviceDialog } from './device-dialog'
import { deviceTitle } from './device-kinds'
import { DeviceRow, ROW_GRID } from './device-row'

type Filter = 'all' | 'online'
type Sort = 'status' | 'name' | 'ip' | 'maker' | 'firstSeen'

const SORTS: Record<Sort, string> = {
  status: 'Status',
  name: 'Name',
  ip: 'IP address',
  maker: 'Maker',
  firstSeen: 'First seen',
}

const ipValue = (ip: string) =>
  ip.split('.').reduce((total, part) => total * 256 + Number(part), 0)

const COMPARE: Record<Exclude<Sort, 'status'>, (a: Device, b: Device) => number> = {
  name: (a, b) => deviceTitle(a).localeCompare(deviceTitle(b)),
  ip: (a, b) => ipValue(a.ip) - ipValue(b.ip),
  maker: (a, b) => (a.vendor ?? '~').localeCompare(b.vendor ?? '~'),
  firstSeen: (a, b) => b.firstSeen - a.firstSeen,
}

export function DevicesView() {
  const { snapshot, progress, scanning, newCount, refresh, scan, acknowledge, forget } =
    useDevices()
  const { notify } = useVpn()
  const [filter, setFilter] = useState<Filter>('all')
  const [sort, setSort] = useState<Sort>('status')
  const [query, setQuery] = useState('')
  const [editing, setEditing] = useState<{ device: Device; open: boolean } | null>(null)

  // Looked up again on every visit: the laptop may be on another network now.
  useEffect(() => {
    void refresh()
  }, [refresh])

  // Newcomers stay marked while the page is open, and count as seen once the
  // user leaves it - so the badge on the tab clears without a chore.
  const hadNew = useRef(false)
  useEffect(() => {
    hadNew.current = newCount > 0
  }, [newCount])
  useEffect(
    () => () => {
      if (hadNew.current) void acknowledge(null)
    },
    [acknowledge],
  )

  const devices = useMemo(() => snapshot?.devices ?? [], [snapshot])
  const online = devices.filter((device) => device.online).length

  const shown = useMemo(() => {
    const needle = query.trim().toLowerCase()
    const matching = devices.filter((device) => {
      if (filter === 'online' && !device.online) return false
      if (!needle) return true
      return [deviceTitle(device), device.ip, device.mac, device.vendor, device.hostname]
        .filter(Boolean)
        .some((field) => field!.toLowerCase().includes(needle))
    })
    // `status` is the backend's order: this PC, the router, then who is here.
    return sort === 'status' ? matching : [...matching].sort(COMPARE[sort])
  }, [devices, filter, query, sort])

  const copy = (what: string, value: string) => {
    void navigator.clipboard
      .writeText(value)
      .then(() => notify({ level: 'info', message: `Copied the ${what}.` }))
      .catch(() => notify({ level: 'error', message: `Couldn't copy the ${what}.` }))
  }

  const network = snapshot?.network ?? null
  const percent = progress && progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <header className="flex shrink-0 items-center justify-between gap-4 px-8 pt-1 pb-4">
        <div className="min-w-0">
          <h1 className="font-heading text-xl font-semibold">Devices</h1>
          <p className="truncate text-xs text-muted-foreground">
            {network ? (
              <>
                {network.adapter} <span className="text-muted-foreground/50">·</span>{' '}
                <span className="numeric" data-selectable>
                  {network.subnet}
                </span>{' '}
                <span className="text-muted-foreground/50">·</span> this PC is{' '}
                <span className="numeric" data-selectable>
                  {network.address}
                </span>
              </>
            ) : (
              'Everything else on your local network.'
            )}
          </p>
        </div>

        <div className="flex shrink-0 items-center gap-3">
          {snapshot?.lastScan && !scanning && (
            <span className="text-xs text-muted-foreground">
              Scanned {formatAgo(snapshot.lastScan)}
            </span>
          )}
          <Button size="sm" onClick={() => void scan()} disabled={scanning || !network}>
            <HugeiconsIcon
              icon={Radar01Icon}
              strokeWidth={2}
              className={scanning ? 'animate-pulse' : undefined}
            />
            {scanning ? (progress?.total ? `Scanning ${percent}%` : 'Scanning…') : 'Scan now'}
          </Button>
        </div>
      </header>

      {network && devices.length > 0 && (
        <div className="flex shrink-0 items-center gap-3 px-8 pb-3">
          <Segmented
            options={[
              { value: 'all' as const, label: `All ${devices.length}` },
              { value: 'online' as const, label: `Online ${online}` },
            ]}
            value={filter}
            onChange={setFilter}
            layoutId="devices-filter"
            size="sm"
            aria-label="Devices shown"
          />
          {newCount > 0 && (
            <button
              type="button"
              onClick={() => void acknowledge(null)}
              className="flex h-7 items-center gap-1.5 rounded-full bg-primary/12 px-2.5 text-[11px] font-semibold text-primary transition-colors outline-none hover:bg-primary/18 focus-visible:ring-2 focus-visible:ring-primary/50"
            >
              <HugeiconsIcon icon={Tick02Icon} size={12} strokeWidth={2.4} />
              {newCount} new · mark as seen
            </button>
          )}

          <div className="ml-auto flex items-center gap-2">
            <Select items={SORTS} value={sort} onValueChange={(value) => setSort((value as Sort) ?? 'status')}>
              <SelectTrigger size="sm" aria-label="Sort by" className="min-w-30 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {(Object.keys(SORTS) as Sort[]).map((value) => (
                  <SelectItem key={value} value={value}>
                    {SORTS[value]}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <div className="relative w-52">
              <HugeiconsIcon
                icon={Search01Icon}
                size={14}
                strokeWidth={2}
                className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-muted-foreground"
              />
              <Input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Name, IP, MAC or maker"
                aria-label="Search devices"
                className="h-8 pl-8 text-xs"
              />
            </div>
          </div>
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto px-8 pb-6">
        <div className="panel relative overflow-hidden rounded-2xl">
          {/* Sweep progress, along the top edge of the list it is filling. */}
          <AnimatePresence>
            {scanning && (
              <motion.div
                key="progress"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                transition={transition.fast}
                className="absolute inset-x-0 top-0 h-0.5 bg-tint/6"
              >
                <div
                  className={cn(
                    'h-full bg-primary transition-[width] duration-300 ease-out',
                    !progress?.total && 'w-1/4 animate-pulse',
                  )}
                  style={progress?.total ? { width: `${Math.max(4, percent)}%` } : undefined}
                />
              </motion.div>
            )}
          </AnimatePresence>

          {!snapshot ? (
            <Empty title="Looking for your network…" />
          ) : !snapshot.supported ? (
            <Empty title="Device scanning needs Windows" body="This platform isn't supported yet." />
          ) : !network ? (
            <Empty
              icon={WifiDisconnected02Icon}
              title="No local network"
              body="Connect to Wi-Fi or Ethernet to see what else is on it. A VPN or mobile-data link alone has no neighbours to find."
            />
          ) : devices.length === 0 ? (
            <Empty
              icon={Radar01Icon}
              title={scanning ? `Scanning ${network.subnet}…` : 'Not scanned yet'}
              body={
                scanning
                  ? 'Asking every address on the network who is there. This takes a few seconds.'
                  : "Scan to find every device on this network - phones, TVs, printers, and anything you didn't know was there."
              }
            />
          ) : (
            <>
              <div
                className={cn(
                  ROW_GRID,
                  'border-b border-hairline px-4 py-2 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase',
                )}
              >
                <span>Device</span>
                <span>Maker</span>
                <span>Status</span>
                <span>First seen</span>
                <span />
              </div>
              {shown.length === 0 ? (
                <Empty
                  title={filter === 'online' && !query ? 'Nobody online right now' : 'No matches'}
                  body={
                    filter === 'online' && !query
                      ? 'Scan again to check who is here, or show all devices.'
                      : 'Try another name, address or maker.'
                  }
                />
              ) : (
                <motion.ol
                  variants={staggerParent}
                  initial="initial"
                  animate="animate"
                  className="divide-y divide-tint/5"
                >
                  {shown.map((device) => (
                    <DeviceRow
                      key={device.mac}
                      device={device}
                      onEdit={() => setEditing({ device, open: true })}
                      onCopy={copy}
                      onAcknowledge={() => void acknowledge(device.mac)}
                      onForget={() => void forget(device.mac)}
                    />
                  ))}
                </motion.ol>
              )}
            </>
          )}
        </div>

        {network && devices.length > 0 && (
          <p className="mt-3 px-1 text-xs leading-relaxed text-muted-foreground">
            Found with ARP, which every device on the network has to answer, and named by asking
            each one directly. Nothing leaves your network.
          </p>
        )}
      </div>

      <DeviceDialog
        device={editing?.device ?? null}
        open={editing?.open ?? false}
        // Closed in place, so the dialog animates out with its content intact.
        onOpenChange={(open) => setEditing((current) => (current ? { ...current, open } : null))}
      />
    </div>
  )
}

function Empty({
  icon = Radar01Icon,
  title,
  body,
}: {
  icon?: typeof Radar01Icon
  title: string
  body?: string
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 px-10 py-16 text-center">
      <span className="grid size-12 place-items-center rounded-2xl bg-tint/7 text-muted-foreground">
        <HugeiconsIcon icon={icon} size={22} strokeWidth={1.8} />
      </span>
      <p className="text-sm font-semibold">{title}</p>
      {body && <p className="max-w-sm text-xs leading-relaxed text-muted-foreground">{body}</p>}
    </div>
  )
}
