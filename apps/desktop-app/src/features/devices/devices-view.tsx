/**
 * Devices: everything else on this network - who it is, who made it, whether
 * it is here now, and which ones are new - as one flat table under the
 * page's toolbar.
 */

import { useEffect, useMemo, useRef, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowDown01Icon,
  Radar01Icon,
  Search01Icon,
  Tick02Icon,
  WifiDisconnected02Icon,
} from '@hugeicons/core-free-icons'
import { AnimatePresence, motion } from 'motion/react'
import { cn } from 'cn'

import { Toolbar, ToolbarButton, ToolbarChoice } from '@/components/toolbar'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Input } from '@/components/ui/input'
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
  const listed = network !== null && devices.length > 0
  const percent = progress && progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <Toolbar
        start={
          listed ? (
            <>
              <ToolbarChoice
                options={[
                  { value: 'all' as const, label: `All ${devices.length}` },
                  { value: 'online' as const, label: `Online ${online}` },
                ]}
                value={filter}
                onChange={setFilter}
                layoutId="devices-filter"
                aria-label="Devices shown"
              />
              {newCount > 0 && (
                <ToolbarButton
                  onClick={() => void acknowledge(null)}
                  className="text-primary hover:text-primary"
                >
                  <HugeiconsIcon icon={Tick02Icon} size={12} strokeWidth={2.4} />
                  {newCount} new · mark as seen
                </ToolbarButton>
              )}
            </>
          ) : null
        }
        center={
          network ? (
            <span className="truncate text-xs text-muted-foreground" title={`Gateway ${network.gateway}`}>
              {network.adapter} <span className="text-muted-foreground/50">·</span>{' '}
              <span className="numeric" data-selectable>
                {network.subnet}
              </span>
            </span>
          ) : null
        }
        end={
          <>
            {listed && (
              <>
                <SortMenu sort={sort} onChange={setSort} />
                <div className="relative w-36">
                  <HugeiconsIcon
                    icon={Search01Icon}
                    size={13}
                    strokeWidth={2}
                    className="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-muted-foreground"
                  />
                  <Input
                    value={query}
                    onChange={(event) => setQuery(event.target.value)}
                    placeholder="Search devices"
                    aria-label="Search devices by name, IP, MAC or maker"
                    className="h-6 rounded-[3px] pl-7 text-xs"
                  />
                </div>
              </>
            )}
            <Button
              size="xs"
              className="ml-1"
              onClick={() => void scan()}
              disabled={scanning || !network}
              title="Asks every address on the network who is there, with ARP. Nothing leaves your network."
            >
              <HugeiconsIcon
                icon={Radar01Icon}
                strokeWidth={2}
                className={scanning ? 'animate-pulse' : undefined}
              />
              {scanning ? (progress?.total ? `Scanning ${percent}%` : 'Scanning…') : 'Scan'}
            </Button>
          </>
        }
      />

      <div className="relative min-h-0 flex-1 overflow-y-auto">
        {/* Sweep progress, along the top edge of the list it is filling. */}
        <AnimatePresence>
          {scanning && (
            <motion.div
              key="progress"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={transition.fast}
              className="absolute inset-x-0 top-0 z-20 h-0.5 bg-tint/6"
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
                : "Scan to find every device on this network - phones, TVs, printers, and anything you didn't know was there. Found with ARP, which every device has to answer; nothing leaves your network."
            }
          />
        ) : (
          <>
            <div
              className={cn(
                ROW_GRID,
                'sticky top-0 z-10 h-7 border-b border-hairline bg-background px-3 text-[11px] font-medium text-muted-foreground',
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
                className="divide-y divide-hairline"
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

      {/* A status line along the foot, as GlassWire keeps its own. */}
      {listed && (
        <p className="h-6 shrink-0 truncate border-t border-hairline bg-rail/60 px-3 text-[11px] leading-6 text-muted-foreground">
          {snapshot?.lastScan && !scanning ? `Last scanned ${formatAgo(snapshot.lastScan)}` : 'Scanning'}
          <span className="text-muted-foreground/50"> · </span>
          this PC is{' '}
          <span className="numeric" data-selectable>
            {network.address}
          </span>
        </p>
      )}

      <DeviceDialog
        device={editing?.device ?? null}
        open={editing?.open ?? false}
        // Closed in place, so the dialog animates out with its content intact.
        onOpenChange={(open) => setEditing((current) => (current ? { ...current, open } : null))}
      />
    </div>
  )
}

function SortMenu({ sort, onChange }: { sort: Sort; onChange: (sort: Sort) => void }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={<ToolbarButton aria-label={`Sort by ${SORTS[sort]}`} />}>
        <span className="text-muted-foreground">Sort:</span>
        {SORTS[sort]}
        <HugeiconsIcon icon={ArrowDown01Icon} size={12} strokeWidth={2} className="text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-40 min-w-40">
        <DropdownMenuRadioGroup value={sort} onValueChange={(value: Sort) => onChange(value)}>
          {(Object.keys(SORTS) as Sort[]).map((value) => (
            <DropdownMenuRadioItem closeOnClick key={value} value={value}>
              {SORTS[value]}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
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
    <div className="flex flex-col items-center justify-center gap-3 px-10 py-14 text-center">
      <span className="grid size-11 place-items-center rounded-lg bg-tint/7 text-muted-foreground">
        <HugeiconsIcon icon={icon} size={22} strokeWidth={1.8} />
      </span>
      <p className="text-sm font-semibold">{title}</p>
      {body && <p className="max-w-sm text-xs leading-relaxed text-muted-foreground">{body}</p>}
    </div>
  )
}
