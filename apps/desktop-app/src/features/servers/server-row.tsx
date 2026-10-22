/** One server in the list: pick it, see how it is reached, act on it. */

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
  onShare: () => void
  onEdit: () => void
  onRemove: () => void
}

export function ServerRow({
  server,
  selected,
  inUse,
  onSelect,
  onShare,
  onEdit,
  onRemove,
}: ServerRowProps) {
  const chips = protocolChips(server)

  const country = regionCode(server.countryCode)

  return (
    <RowFrame selected={selected} label={`Use ${server.name}`} onSelect={onSelect}>
      {/* Where it is, when known; otherwise a server tile in the same slot. */}
      {country ? (
        <span className="pointer-events-none grid size-8 shrink-0 place-items-center rounded-xl bg-tint/6">
          <Flag code={country} className="h-3.5 w-5" />
        </span>
      ) : (
        <span
          className="tone-tile pointer-events-none grid size-8 shrink-0 place-items-center rounded-xl"
          style={toneStyle('violet')}
        >
          <HugeiconsIcon icon={ServerStack01Icon} size={16} strokeWidth={2} />
        </span>
      )}
      <div className="pointer-events-none min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5">
          <span className="truncate text-sm font-medium">{server.name}</span>
          {/* One pill, the full stack on hover: every supported server is
              TCP + TLS today, so three chips would only crowd the address. */}
          <span
            title={chips.join(' · ')}
            className="pointer-events-auto shrink-0 rounded-md bg-tint/7 px-1.5 py-px text-[9px] font-semibold tracking-wide text-muted-foreground uppercase"
          >
            {chips[0]}
          </span>
          {server.allowInsecure && (
            <span
              title="Certificate checks are off for this server"
              className="pointer-events-auto shrink-0 rounded-md bg-warning/12 px-1.5 py-px text-[9px] font-semibold tracking-wide text-warning uppercase"
            >
              Unverified
            </span>
          )}
          {inUse && (
            <span className="shrink-0 rounded-full bg-protected/14 px-1.5 py-px text-[10px] font-semibold text-protected">
              Connected
            </span>
          )}
        </div>
        <span className="mt-0.5 block truncate font-mono text-[11px] text-muted-foreground">
          {server.host}:{server.port}
        </span>
      </div>

      <LatencyBadge ms={server.latencyMs} />

      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={`More actions for ${server.name}`}
              className="relative z-10 text-muted-foreground opacity-60 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 data-popup-open:opacity-100"
            />
          }
        >
          <HugeiconsIcon icon={MoreVerticalIcon} strokeWidth={2} />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-44">
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
}: {
  selected: boolean
  fastest: ServerSummary | null
  onSelect: () => void
}) {
  return (
    <RowFrame selected={selected} label="Choose the fastest server automatically" onSelect={onSelect}>
      <span
        className="tone-tile pointer-events-none grid size-8 shrink-0 place-items-center rounded-xl"
        style={toneStyle('amber')}
      >
        <HugeiconsIcon icon={FlashIcon} size={16} strokeWidth={2} />
      </span>
      <div className="pointer-events-none min-w-0 flex-1">
        <span className="block text-sm font-medium">Automatic</span>
        <span className="block truncate text-[11px] text-muted-foreground">
          {fastest ? `Fastest right now: ${fastest.name}` : 'Picks the fastest server when you connect'}
        </span>
      </div>
      {fastest && <LatencyBadge ms={fastest.latencyMs} />}
      {/* Keeps the badge column aligned with the rows' menu buttons. */}
      <span aria-hidden className="size-8 shrink-0" />
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
  children,
}: {
  selected: boolean
  label: string
  onSelect: () => void
  children: ReactNode
}) {
  return (
    <motion.li variants={staggerChild} layout="position" transition={spring.soft}>
      <div
        className={cn(
          'group relative flex items-center gap-3 rounded-2xl border py-2.5 pr-1.5 pl-3 transition-colors duration-200',
          // The radio says which; the tint repeats it at a glance down the list.
          selected ? 'border-selected-border bg-selected' : 'border-transparent hover:bg-tint/4',
        )}
      >
        <button
          type="button"
          role="radio"
          aria-checked={selected}
          aria-label={label}
          onClick={onSelect}
          className="absolute inset-0 rounded-2xl outline-none focus-visible:ring-2 focus-visible:ring-primary/50"
        />
        <span
          aria-hidden
          className={cn(
            'pointer-events-none grid size-4 shrink-0 place-items-center rounded-full border-2 transition-colors duration-200',
            selected ? 'border-primary' : 'border-tint/20 group-hover:border-tint/35',
          )}
        >
          {selected && (
            <motion.span
              layoutId="server-selected"
              className="size-1.5 rounded-full bg-primary"
              transition={spring.soft}
            />
          )}
        </span>
        {children}
      </div>
    </motion.li>
  )
}

// Traffic-light tones: the number alone does not tell you whether 140 ms is good.
const LATENCY_TONE = {
  good: 'bg-success/12 text-success',
  fair: 'bg-warning/12 text-warning',
  poor: 'bg-destructive/12 text-destructive',
  unknown: 'bg-tint/6 text-muted-foreground',
} as const

function LatencyBadge({ ms }: { ms: number | null }) {
  return (
    <span
      className={cn(
        'numeric pointer-events-none w-14 shrink-0 rounded-full px-2 py-0.5 text-center text-[11px] font-semibold',
        LATENCY_TONE[latencyQuality(ms)],
      )}
    >
      {ms === null ? '—' : `${ms} ms`}
    </span>
  )
}
