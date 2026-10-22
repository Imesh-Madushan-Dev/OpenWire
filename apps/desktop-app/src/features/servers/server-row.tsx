/**
 * One server in the list: pick it, see how it is reached, act on it.
 *
 * Flat rows in GlassWire's manner - the chosen one lit and marked with an
 * accent bar down its left edge. Click chooses; double-click chooses and
 * connects.
 */

import type { ReactNode } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Delete02Icon,
  FlashIcon,
  MoreVerticalIcon,
  PencilEdit02Icon,
  QrCodeIcon,
  ServerStack01Icon,
} from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'
import { cn } from 'cn'

import { Flag } from '@/components/flag'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { latencyQuality, regionCode } from '@/lib/format'
import { spring, staggerChild } from '@/lib/motion'
import { toneStyle } from '@/lib/tone'
import type { ServerSummary } from '@/lib/types'

import { protocolChips } from './links'

interface ServerRowProps {
  server: ServerSummary
  selected: boolean
  inUse: boolean
  onSelect: () => void
  onConnect: () => void
  onShare: () => void
  onEdit: () => void
  onRemove: () => void
}

export function ServerRow({
  server,
  selected,
  inUse,
  onSelect,
  onConnect,
  onShare,
  onEdit,
  onRemove,
}: ServerRowProps) {
  const chips = protocolChips(server)
  const country = regionCode(server.countryCode)

  return (
    <RowFrame
      selected={selected}
      label={`Use ${server.name}`}
      onSelect={onSelect}
      onConnect={onConnect}
    >
      {/* Where it is, when known; otherwise a server tile in the same slot. */}
      {country ? (
        <span className="pointer-events-none grid w-6 shrink-0 place-items-center">
          <Flag code={country} className="h-3.5 w-5" />
        </span>
      ) : (
        <span
          className="tone-tile pointer-events-none grid size-6 shrink-0 place-items-center rounded-sm"
          style={toneStyle('violet')}
        >
          <HugeiconsIcon icon={ServerStack01Icon} size={13} strokeWidth={2} />
        </span>
      )}
      <div className="pointer-events-none min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5">
          <span className={cn('truncate text-[13px]', selected ? 'font-semibold' : 'font-medium')}>
            {server.name}
          </span>
          {inUse && (
            <span className="shrink-0 rounded-[3px] bg-protected/16 px-1 text-[10px] font-semibold text-protected">
              Connected
            </span>
          )}
          {server.allowInsecure && (
            <span
              title="Certificate checks are off for this server"
              className="pointer-events-auto shrink-0 rounded-[3px] bg-warning/14 px-1 text-[10px] font-semibold text-warning"
            >
              Unverified
            </span>
          )}
        </div>
        {/* The full stack on hover: every supported server is TCP + TLS
            today, so chips would only crowd the address. */}
        <span
          title={chips.join(' · ')}
          className="pointer-events-auto block truncate font-mono text-[11px] text-muted-foreground"
        >
          {server.host}
        </span>
      </div>

      <Latency ms={server.latencyMs} />

      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label={`More actions for ${server.name}`}
              className="relative z-10 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 data-popup-open:opacity-100"
            />
          }
        >
          <HugeiconsIcon icon={MoreVerticalIcon} strokeWidth={2} />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-44">
          <DropdownMenuItem onClick={onConnect}>
            <HugeiconsIcon icon={FlashIcon} strokeWidth={2} />
            {inUse ? 'Reconnect' : 'Connect'}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={onShare}>
            <HugeiconsIcon icon={QrCodeIcon} strokeWidth={2} />
            Share
          </DropdownMenuItem>
          <DropdownMenuItem onClick={onEdit}>
            <HugeiconsIcon icon={PencilEdit02Icon} strokeWidth={2} />
            Edit
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem variant="destructive" onClick={onRemove}>
            <HugeiconsIcon icon={Delete02Icon} strokeWidth={2} />
            Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </RowFrame>
  )
}

/** The list's first entry: let Maizy pick the fastest server at connect time. */
export function AutomaticRow({
  selected,
  fastest,
  onSelect,
  onConnect,
}: {
  selected: boolean
  fastest: ServerSummary | null
  onSelect: () => void
  onConnect: () => void
}) {
  return (
    <RowFrame
      selected={selected}
      label="Choose the fastest server automatically"
      onSelect={onSelect}
      onConnect={onConnect}
    >
      <span
        className="tone-tile pointer-events-none grid size-6 shrink-0 place-items-center rounded-sm"
        style={toneStyle('amber')}
      >
        <HugeiconsIcon icon={FlashIcon} size={13} strokeWidth={2} />
      </span>
      <div className="pointer-events-none min-w-0 flex-1">
        <span className={cn('block text-[13px]', selected ? 'font-semibold' : 'font-medium')}>
          Fastest server
        </span>
        <span className="block truncate text-[11px] text-muted-foreground">
          {fastest ? `Right now: ${fastest.name}` : 'Picked when you connect'}
        </span>
      </div>
      {fastest && <Latency ms={fastest.latencyMs} />}
      {/* Keeps the latency column aligned with the rows' menu buttons. */}
      <span aria-hidden className="size-6 shrink-0" />
    </RowFrame>
  )
}

/**
 * The selectable shell shared by every row. The whole row is the radio;
 * anything interactive inside sits above it with `relative z-10`.
 */
function RowFrame({
  selected,
  label,
  onSelect,
  onConnect,
  children,
}: {
  selected: boolean
  label: string
  onSelect: () => void
  onConnect: () => void
  children: ReactNode
}) {
  return (
    <motion.li variants={staggerChild} layout="position" transition={spring.soft}>
      <div
        className={cn(
          'group relative flex h-11 items-center gap-2.5 pr-1.5 pl-3 transition-colors duration-150',
          selected ? 'bg-selected' : 'hover:bg-tint/4',
        )}
      >
        <button
          type="button"
          role="radio"
          aria-checked={selected}
          aria-label={label}
          title="Double-click to connect"
          onClick={onSelect}
          onDoubleClick={onConnect}
          className="absolute inset-0 outline-none focus-visible:ring-2 focus-visible:ring-primary/50 focus-visible:ring-inset"
        />
        {selected && (
          <motion.span
            layoutId="server-selected"
            aria-hidden
            className="pointer-events-none absolute inset-y-0 left-0 w-0.5 bg-primary"
            transition={spring.soft}
          />
        )}
        {children}
      </div>
    </motion.li>
  )
}

// Traffic-light tones: the number alone does not tell you whether 140 ms is good.
const LATENCY_TONE = {
  good: 'text-success',
  fair: 'text-warning',
  poor: 'text-destructive',
  unknown: 'text-muted-foreground',
} as const

function Latency({ ms }: { ms: number | null }) {
  return (
    <span
      className={cn(
        'numeric pointer-events-none w-12 shrink-0 text-right text-[11px] font-semibold',
        LATENCY_TONE[latencyQuality(ms)],
      )}
    >
      {ms === null ? '—' : `${ms} ms`}
    </span>
  )
}
