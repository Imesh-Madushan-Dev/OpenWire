/** One device in the list: what it is, who made it, whether it is here now. */

import { HugeiconsIcon } from '@hugeicons/react'
import {
  Copy01Icon,
  Delete02Icon,
  MoreVerticalIcon,
  PencilEdit02Icon,
  Tick02Icon,
} from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'
import { cn } from 'cn'

import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { formatAgo, formatDay } from '@/lib/format'
import { staggerChild } from '@/lib/motion'
import type { Device } from '@/lib/types'

import { deviceTitle, KINDS } from './device-kinds'

/** Shared with the header row, so the columns line up. */
export const ROW_GRID =
  'grid grid-cols-[minmax(0,1.8fr)_minmax(0,1fr)_6rem_4.5rem_1.75rem] items-center gap-3'

const ROLE_LABEL = { thisDevice: 'This PC', router: 'Router' } as const

interface DeviceRowProps {
  device: Device
  onEdit: () => void
  onCopy: (what: string, value: string) => void
  onAcknowledge: () => void
  onForget: () => void
}

export function DeviceRow({ device, onEdit, onCopy, onAcknowledge, onForget }: DeviceRowProps) {
  const kind = KINDS[device.kind]

  return (
    <motion.li
      variants={staggerChild}
      layout="position"
      className={cn(
        ROW_GRID,
        'group h-11 px-3 transition-colors duration-150 hover:bg-tint/4',
        // A faint wash until looked at, so newcomers stand out in a long list.
        device.isNew && 'bg-primary/6 hover:bg-primary/9',
      )}
    >
      <div className="flex min-w-0 items-center gap-3">
        <span
          aria-hidden
          className={cn(
            'grid size-7 shrink-0 place-items-center rounded-md bg-tint/6 transition-colors',
            device.online ? 'text-foreground/85' : 'text-muted-foreground/60',
          )}
        >
          <HugeiconsIcon icon={kind.icon} size={15} strokeWidth={1.8} />
        </span>
        <div className="min-w-0">
          <div className="flex min-w-0 items-center gap-1.5">
            <span
              className={cn('truncate text-[13px] font-medium', !device.online && 'text-foreground/70')}
              title={device.hostname && device.customName ? `Calls itself ${device.hostname}` : undefined}
            >
              {deviceTitle(device)}
            </span>
            {device.role && (
              <span className="shrink-0 rounded-[3px] bg-tint/8 px-1 text-[9px] font-semibold tracking-wide text-muted-foreground uppercase">
                {ROLE_LABEL[device.role]}
              </span>
            )}
            {device.isNew && (
              <span className="shrink-0 rounded-[3px] bg-primary/15 px-1 text-[10px] font-semibold text-primary">
                New
              </span>
            )}
          </div>
          <span
            className="block truncate font-mono text-[11px] text-muted-foreground"
            data-selectable
          >
            {device.ip} <span className="text-muted-foreground/50">·</span> {device.mac}
          </span>
        </div>
      </div>

      <div className="min-w-0">
        <Maker device={device} />
        <span className="block truncate text-[11px] text-muted-foreground">{kind.label}</span>
      </div>

      <div className="flex min-w-0 items-center gap-1.5 text-xs">
        <span
          aria-hidden
          className={cn('size-1.5 shrink-0 rounded-full', device.online ? 'bg-success' : 'bg-tint/25')}
        />
        <span
          className={cn('truncate', device.online ? 'text-foreground/85' : 'text-muted-foreground')}
          title={device.online ? undefined : new Date(device.lastSeen * 1000).toLocaleString()}
        >
          {device.online ? 'Online' : formatAgo(device.lastSeen)}
        </span>
      </div>

      <span
        className="numeric truncate text-xs text-muted-foreground"
        title={new Date(device.firstSeen * 1000).toLocaleString()}
      >
        {formatDay(device.firstSeen)}
      </span>

      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label={`Actions for ${deviceTitle(device)}`}
              className="text-muted-foreground opacity-60 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 data-popup-open:opacity-100"
            />
          }
        >
          <HugeiconsIcon icon={MoreVerticalIcon} strokeWidth={2} />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-48">
          <DropdownMenuItem onClick={onEdit}>
            <HugeiconsIcon icon={PencilEdit02Icon} strokeWidth={2} />
            Rename or change type
          </DropdownMenuItem>
          {device.isNew && (
            <DropdownMenuItem onClick={onAcknowledge}>
              <HugeiconsIcon icon={Tick02Icon} strokeWidth={2} />
              Mark as seen
            </DropdownMenuItem>
          )}
          <DropdownMenuItem onClick={() => onCopy('IP address', device.ip)}>
            <HugeiconsIcon icon={Copy01Icon} strokeWidth={2} />
            Copy IP address
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => onCopy('MAC address', device.mac)}>
            <HugeiconsIcon icon={Copy01Icon} strokeWidth={2} />
            Copy MAC address
          </DropdownMenuItem>
          {/* This machine and the router are found again on every sweep. */}
          {!device.role && (
            <>
              <DropdownMenuSeparator />
              <DropdownMenuItem variant="destructive" onClick={onForget}>
                <HugeiconsIcon icon={Delete02Icon} strokeWidth={2} />
                Forget device
              </DropdownMenuItem>
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
    </motion.li>
  )
}

function Maker({ device }: { device: Device }) {
  if (device.vendor) {
    return <span className="block truncate text-xs">{device.vendor}</span>
  }
  if (device.privateAddress) {
    return (
      <Tooltip>
        <TooltipTrigger
          render={
            <span className="block w-fit max-w-full cursor-help truncate text-xs text-foreground/80 underline decoration-tint/30 decoration-dotted underline-offset-2" />
          }
        >
          Private address
        </TooltipTrigger>
        <TooltipContent className="max-w-64">
          It uses a randomised Wi-Fi address, so its maker can&apos;t be looked up. Phones do this
          by default.
        </TooltipContent>
      </Tooltip>
    )
  }
  return <span className="block truncate text-xs text-muted-foreground">Unknown maker</span>
}
