/** Location picker. Opens from the bottom, over the dashboard. */

import { useMemo, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Crown02Icon,
  Delete02Icon,
  RefreshIcon,
  Search01Icon,
  Tick02Icon,
} from '@hugeicons/core-free-icons'
import { AnimatePresence, motion } from 'motion/react'
import { cn } from 'cn'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet'
import { formatLatency, latencyQuality } from '@/lib/format'
import { staggerChild, staggerParent } from '@/lib/motion'
import type { ServerSummary } from '@/lib/types'
import { RegionChip } from '@/features/vpn/components/region-chip'
import { useVpn } from '@/features/vpn/vpn-context'

import { ServerImportField } from './server-import-field'

interface ServerSheetProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

const DOT_TONE = {
  good: 'bg-primary',
  fair: 'bg-warning',
  poor: 'bg-destructive',
  unknown: 'bg-muted-foreground/40',
} as const

export function ServerSheet({ open, onOpenChange }: ServerSheetProps) {
  const { servers, settings, connection, chooseServer, probeServers, probing, removeServer } =
    useVpn()
  const [query, setQuery] = useState('')

  const results = useMemo(() => {
    const needle = query.trim().toLowerCase()
    const matches = needle
      ? servers.filter((server) =>
          [server.name, server.country, server.city, ...server.tags]
            .join(' ')
            .toLowerCase()
            .includes(needle),
        )
      : servers

    // Usable servers first, then fastest. Unmeasured nodes sort last rather
    // than pretending to be instant.
    return [...matches].sort((a, b) => {
      if (a.configured !== b.configured) return a.configured ? -1 : 1
      return (a.latencyMs ?? Number.MAX_SAFE_INTEGER) - (b.latencyMs ?? Number.MAX_SAFE_INTEGER)
    })
  }, [servers, query])

  const selectedId = settings?.preferredServer ?? null
  const hasConfigured = servers.some((server) => server.configured)

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent
        side="bottom"
        className="mx-auto max-h-[78vh] gap-0 rounded-t-3xl rounded-b-xl border-white/10 p-0 sm:max-w-none"
      >
        <SheetHeader className="gap-3 px-5 pt-5 pb-3">
          <div className="flex items-start justify-between gap-4">
            <div>
              <SheetTitle className="text-base">Choose a location</SheetTitle>
              <SheetDescription className="text-xs">
                Sorted by measured response time.
              </SheetDescription>
            </div>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void probeServers()}
              disabled={probing || !hasConfigured}
              className="mr-9 shrink-0"
            >
              <HugeiconsIcon
                icon={RefreshIcon}
                strokeWidth={2}
                className={probing ? 'animate-spin' : undefined}
              />
              {probing ? 'Testing' : 'Test speeds'}
            </Button>
          </div>

          <div className="relative">
            <HugeiconsIcon
              icon={Search01Icon}
              size={15}
              strokeWidth={2}
              className="pointer-events-none absolute top-1/2 left-3.5 -translate-y-1/2 text-muted-foreground"
            />
            <Input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search countries and cities"
              aria-label="Search locations"
              className="h-10 pl-9"
            />
          </div>
        </SheetHeader>

        <motion.ul
          variants={staggerParent}
          initial="initial"
          animate="animate"
          className="flex-1 overflow-y-auto px-3 pb-2"
        >
          <AnimatePresence initial={false}>
            {results.map((server) => (
              <ServerRow
                key={server.id}
                server={server}
                selected={server.id === selectedId}
                active={server.id === connection.serverId}
                onSelect={() => {
                  void chooseServer(server.id)
                  onOpenChange(false)
                }}
                onRemove={
                  server.tags.includes('custom')
                    ? () => void removeServer(server.id)
                    : undefined
                }
              />
            ))}
          </AnimatePresence>

          {results.length === 0 && (
            <li className="px-3 py-10 text-center text-sm text-muted-foreground">
              No locations match "{query}".
            </li>
          )}
        </motion.ul>

        <div className="border-t border-white/8 px-5 py-4">
          <p className="mb-2 text-xs font-medium text-foreground/80">Add your own server</p>
          <ServerImportField />
        </div>
      </SheetContent>
    </Sheet>
  )
}

interface ServerRowProps {
  server: ServerSummary
  selected: boolean
  active: boolean
  onSelect: () => void
  onRemove?: () => void
}

function ServerRow({ server, selected, active, onSelect, onRemove }: ServerRowProps) {
  const quality = latencyQuality(server.latencyMs)

  return (
    <motion.li variants={staggerChild} layout exit={{ opacity: 0, height: 0 }}>
      <div
        className={cn(
          'group flex items-center gap-3 rounded-2xl px-3 py-2.5 transition-colors',
          server.configured ? 'hover:bg-white/5' : 'opacity-55',
        )}
      >
        <button
          type="button"
          onClick={onSelect}
          disabled={!server.configured}
          className="flex min-w-0 flex-1 items-center gap-3 text-left outline-none focus-visible:ring-2 focus-visible:ring-primary/50 focus-visible:ring-offset-0 disabled:cursor-not-allowed"
        >
          <RegionChip countryCode={server.countryCode} />

          <span className="min-w-0 flex-1">
            <span className="flex items-center gap-1.5">
              <span className="truncate text-sm font-medium">{server.name}</span>
              {server.tier === 'premium' && (
                <HugeiconsIcon
                  icon={Crown02Icon}
                  size={12}
                  strokeWidth={2}
                  className="shrink-0 text-warning"
                  aria-label="Premium"
                />
              )}
              {active && (
                <span className="rounded-full bg-primary/15 px-1.5 py-0.5 text-[10px] font-semibold text-primary">
                  In use
                </span>
              )}
            </span>
            <span className="block truncate text-xs text-muted-foreground">
              {server.configured
                ? [server.city || server.country, server.protocol === 'direct' ? 'unmasked' : null]
                    .filter(Boolean)
                    .join(' · ')
                : 'Setup required - add this server with its share link'}
            </span>
          </span>

          {/* Load bar: a quiet capacity hint, not a headline number. */}
          <span aria-hidden className="hidden h-1 w-10 overflow-hidden rounded-full bg-white/8 sm:block">
            <span
              className="block h-full rounded-full bg-white/25"
              style={{ width: `${Math.min(100, Math.max(4, server.load))}%` }}
            />
          </span>

          <span className="numeric flex w-16 items-center justify-end gap-1.5 text-xs text-muted-foreground">
            <span aria-hidden className={cn('size-1.5 rounded-full', DOT_TONE[quality])} />
            {formatLatency(server.latencyMs)}
          </span>

          <span className="grid w-5 place-items-center">
            {selected && (
              <HugeiconsIcon
                icon={Tick02Icon}
                size={16}
                strokeWidth={2.4}
                className="text-primary"
                aria-label="Selected"
              />
            )}
          </span>
        </button>

        {onRemove && (
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={onRemove}
            aria-label={`Remove ${server.name}`}
            className="opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
          >
            <HugeiconsIcon icon={Delete02Icon} strokeWidth={2} className="text-destructive" />
          </Button>
        )}
      </div>
    </motion.li>
  )
}
