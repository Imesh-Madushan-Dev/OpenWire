/**
 * One ranked column of the usage breakdown.
 *
 * Bar length encodes rank against the column's leader; inside each bar the
 * two series split incoming from outgoing, in the same colours as the ring and
 * the live graph. Only the first few rows show until the user asks for more -
 * the long tail is where the noise lives.
 */

import { useState } from 'react'
import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/react'
import { ArrowDown01Icon, ArrowUp01Icon } from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'
import { cn } from 'cn'

import { countryName, Flag } from '@/components/flag'
import { Skeleton } from '@/components/ui/skeleton'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import { formatBytes } from '@/lib/format'
import { staggerChild } from '@/lib/motion'
import type { Ranked, UsageEntry } from '@/lib/types'

/** Rows shown before "+N more". */
const VISIBLE = 8

interface BreakdownColumnProps {
  title: string
  icon: IconSvgElement
  ranked: Ranked
  /** Shown in place of the list when there is nothing to rank. */
  emptyHint: string
  /** Shows placeholder rows while the figures are still arriving. */
  loading?: boolean
  /** One line under the heading, saying where this column's figures come from. */
  note?: string
  /** Per-row glyphs, for the apps column. */
  iconFor?: (key: string) => string | undefined
  /** A country flag beside each row, for the hosts column. */
  flags?: boolean
}

export function BreakdownColumn({
  title,
  icon,
  ranked,
  emptyHint,
  loading = false,
  note,
  iconFor,
  flags = false,
}: BreakdownColumnProps) {
  const [expanded, setExpanded] = useState(false)
  const { entries, more } = ranked

  // Scaled against the leader, not the column total, so the shape of the
  // ranking stays readable when one entry dominates.
  const leader = entries[0]?.total ?? 0
  const distinct = entries.length + (more?.count ?? 0)

  const shown = expanded ? entries : entries.slice(0, VISIBLE)
  const folded = expanded ? [] : entries.slice(VISIBLE)
  // Everything not on screen: the rows behind the fold, plus the tail the
  // backend already summed.
  const rest = fold(folded, expanded ? null : more)

  return (
    <motion.section
      variants={staggerChild}
      className="flex min-w-0 flex-col gap-3 p-4"
    >
      <header className="flex items-center gap-2 text-muted-foreground">
        <HugeiconsIcon icon={icon} size={13} strokeWidth={2} />
        <h3 className="text-xs font-semibold tracking-wide uppercase">{title}</h3>
        {distinct > 0 && (
          <span className="numeric ml-auto rounded-full bg-tint/6 px-1.5 py-px text-[10px] font-medium">
            {distinct}
          </span>
        )}
      </header>

      {note && <p className="-mt-1.5 text-[11px] leading-snug text-muted-foreground/70">{note}</p>}

      {entries.length === 0 ? (
        loading ? (
          <Placeholder withGlyph={Boolean(iconFor) || flags} />
        ) : (
          <p className="py-6 text-center text-xs leading-relaxed text-muted-foreground">
            {emptyHint}
          </p>
        )
      ) : (
        <TooltipProvider delay={250}>
          <ol className="flex flex-col gap-2.5">
            {shown.map((entry) => (
              <Row key={entry.key} entry={entry} leader={leader} iconFor={iconFor} flag={flags} />
            ))}

            {/* The tail the backend summed is listed only when expanded;
                folded, it is part of the "+N more" row below. */}
            {expanded && more && (
              <SummaryRow
                label={`+${more.count} smaller`}
                up={more.up}
                down={more.down}
                leader={leader}
              />
            )}
          </ol>

          {rest && (
            <button
              type="button"
              onClick={() => setExpanded(true)}
              className="group -mx-1.5 -mb-1 rounded-lg px-1.5 py-1 text-left outline-none transition-colors hover:bg-tint/5 focus-visible:ring-2 focus-visible:ring-primary/50"
            >
              <SummaryRow
                as="span"
                label={`+${rest.count} more`}
                up={rest.up}
                down={rest.down}
                leader={leader}
                interactive
              />
            </button>
          )}

          {expanded && entries.length > VISIBLE && (
            <button
              type="button"
              onClick={() => setExpanded(false)}
              className="self-start rounded-md text-[11px] font-medium text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-primary/50"
            >
              Show less
            </button>
          )}
        </TooltipProvider>
      )}
    </motion.section>
  )
}

function Row({
  entry,
  leader,
  iconFor,
  flag,
}: {
  entry: UsageEntry
  leader: number
  iconFor?: (key: string) => string | undefined
  flag: boolean
}) {
  const country = flag ? countryName(entry.country) : undefined

  return (
    <Tooltip>
      <TooltipTrigger
        render={<li />}
        className="flex min-w-0 cursor-default flex-col gap-1.5 outline-none"
      >
        <div className="flex min-w-0 items-center gap-2">
          {iconFor && <Glyph src={iconFor(entry.key)} name={entry.key} />}
          {flag && <Flag code={entry.country} />}
          <span className="truncate text-xs font-medium">{entry.key}</span>
          <span className="numeric ml-auto shrink-0 text-[11px] text-muted-foreground">
            {formatBytes(entry.total)}
          </span>
        </div>
        <SplitBar up={entry.up} down={entry.down} leader={leader} />
      </TooltipTrigger>

      <TooltipContent side="top" align="start" className="gap-3">
        <span className="max-w-48 truncate font-medium">{entry.key}</span>
        {country && <span className="opacity-70">{country}</span>}
        <Direction icon={ArrowDown01Icon} color="var(--chart-down)" bytes={entry.down} />
        <Direction icon={ArrowUp01Icon} color="var(--chart-up)" bytes={entry.up} />
      </TooltipContent>
    </Tooltip>
  )
}

function Direction({
  icon,
  color,
  bytes,
}: {
  icon: IconSvgElement
  color: string
  bytes: number
}) {
  return (
    <span className="numeric flex items-center gap-0.5 whitespace-nowrap">
      <HugeiconsIcon icon={icon} size={12} strokeWidth={2.4} style={{ color }} />
      {formatBytes(bytes)}
    </span>
  )
}

function SummaryRow({
  label,
  up,
  down,
  leader,
  interactive = false,
  as: Tag = 'li',
}: {
  label: string
  up: number
  down: number
  leader: number
  interactive?: boolean
  as?: 'li' | 'span'
}) {
  // Spans throughout: this also renders inside a button, which only takes
  // phrasing content.
  return (
    <Tag className="flex min-w-0 flex-col gap-1.5">
      <span className="flex items-center gap-2">
        <span
          className={cn(
            'text-xs font-medium text-muted-foreground',
            interactive && 'transition-colors group-hover:text-foreground',
          )}
        >
          {label}
        </span>
        <span className="numeric ml-auto text-[11px] text-muted-foreground">
          {formatBytes(up + down)}
        </span>
      </span>
      <SplitBar up={up} down={down} leader={leader} muted />
    </Tag>
  )
}

/**
 * Track, fill and split. The fill grows in once on mount with a transform
 * (no layout work), then follows later refreshes with a width transition.
 */
function SplitBar({
  up,
  down,
  leader,
  muted = false,
}: {
  up: number
  down: number
  leader: number
  muted?: boolean
}) {
  const total = up + down
  // A floor keeps a tiny entry visible as a sliver rather than nothing.
  const width = leader > 0 && total > 0 ? Math.max(2, Math.min(100, (total / leader) * 100)) : 0

  return (
    <span aria-hidden className="block h-1.5 w-full overflow-hidden rounded-full bg-tint/6">
      <span
        className={cn(
          'bar-grow flex h-full gap-px overflow-hidden rounded-full transition-[width] duration-500 ease-out',
          muted && 'opacity-45',
        )}
        style={{ width: `${width}%` }}
      >
        {down > 0 && <span className="h-full bg-chart-down" style={{ flexGrow: down }} />}
        {up > 0 && <span className="h-full bg-chart-up" style={{ flexGrow: up }} />}
      </span>
    </span>
  )
}

/** The app's own icon, or its initial when the executable gave none. */
function Glyph({ src, name }: { src?: string; name: string }) {
  if (src) {
    // Decorative: the name right beside it already says which app.
    return <img src={src} alt="" aria-hidden className="size-4 shrink-0 rounded-[3px]" />
  }
  return (
    <span
      aria-hidden
      className="grid size-4 shrink-0 place-items-center rounded-[4px] bg-tint/8 text-[9px] font-semibold text-muted-foreground uppercase"
    >
      {name.replace(/[^a-z0-9]/gi, '').charAt(0) || '?'}
    </span>
  )
}

function Placeholder({ withGlyph }: { withGlyph: boolean }) {
  return (
    <div role="status" aria-label="Loading" className="flex flex-col gap-3 py-1">
      {[78, 64, 52, 40, 30].map((width) => (
        <div key={width} className="flex flex-col gap-1.5">
          <div className="flex items-center gap-2">
            {withGlyph && <Skeleton className="size-4 rounded-[4px]" />}
            <Skeleton className="h-2.5 rounded-full" style={{ width: `${width}%` }} />
          </div>
          <Skeleton className="h-1.5 rounded-full" style={{ width: `${width + 10}%` }} />
        </div>
      ))}
    </div>
  )
}

/** Sums the off-screen rows and the backend's tail into one figure. */
function fold(rows: UsageEntry[], more: Ranked['more']) {
  const count = rows.length + (more?.count ?? 0)
  if (count === 0) return null

  let up = more?.up ?? 0
  let down = more?.down ?? 0
  for (const row of rows) {
    up += row.up
    down += row.down
  }
  return { count, up, down }
}
