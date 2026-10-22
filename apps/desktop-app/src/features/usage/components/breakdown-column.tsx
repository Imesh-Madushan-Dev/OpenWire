/**
 * One ranked column of the usage breakdown, in GlassWire's dense style: a
 * name and figure per line, with a hairline bar underneath split into
 * incoming and outgoing in the chart colours.
 *
 * Bar length encodes rank against the column's leader. The column shows as
 * many rows as its height fits and folds the rest into a "+N more" line,
 * which expands the column into a scrolling list.
 */

import { useLayoutEffect, useRef, useState } from 'react'
import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/react'
import { GridViewIcon } from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'
import { cn } from 'cn'

import { Flag } from '@/components/flag'
import { SeriesIcon } from '@/components/series-icon'
import { Skeleton } from '@/components/ui/skeleton'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import { countryName } from '@/lib/country'
import { formatBytes } from '@/lib/format'
import { staggerChild } from '@/lib/motion'
import type { Ranked, UsageEntry } from '@/lib/types'

/** Every row, the "+N more" line included, is exactly this tall. */
const ROW_HEIGHT = 24
/** Rows shown before the column has been measured. */
const FALLBACK_ROWS = 8

interface BreakdownColumnProps {
  title: string
  ranked: Ranked
  /** Shown in place of the list when there is nothing to rank. */
  emptyHint: string
  /** Shows placeholder rows while the figures are still arriving. */
  loading?: boolean
  /** One line beside the heading, saying where this column's figures come from. */
  note?: string
  /** Per-row glyphs, for the apps column. */
  iconFor?: (key: string) => string | undefined
  /** A country flag beside each row, for the hosts column. */
  flags?: boolean
  /** A glyph for every row of a column with neither icons nor flags. */
  rowIcon?: IconSvgElement
}

export function BreakdownColumn({
  title,
  ranked,
  emptyHint,
  loading = false,
  note,
  iconFor,
  flags = false,
  rowIcon,
}: BreakdownColumnProps) {
  const [expanded, setExpanded] = useState(false)
  const listRef = useRef<HTMLDivElement>(null)
  const fits = useRowsThatFit(listRef)
  const { entries, more } = ranked

  // Scaled against the leader, not the column total, so the shape of the
  // ranking stays readable when one entry dominates.
  const leader = entries[0]?.total ?? 0

  // Folded, one row is given up to the "+N more" line whenever anything is
  // left over - the backend's tail always is.
  const overflow = entries.length > fits || more !== null
  const visible = expanded ? entries.length : overflow ? Math.max(1, fits - 1) : entries.length
  const shown = entries.slice(0, visible)
  const rest = expanded ? null : fold(entries.slice(visible), more)

  return (
    <motion.section
      variants={staggerChild}
      aria-label={title}
      className="flex min-h-0 min-w-0 flex-col pt-2.5"
    >
      <header className="flex h-6 shrink-0 items-center gap-2 px-3">
        <h3 className="text-xs font-medium text-muted-foreground">{title}</h3>
        {note && (
          <span className="truncate text-[10px] text-muted-foreground/70" title={note}>
            {note}
          </span>
        )}
        {expanded && (
          <button
            type="button"
            onClick={() => setExpanded(false)}
            className="ml-auto shrink-0 rounded-[3px] px-1.5 text-[11px] font-medium text-muted-foreground outline-none transition-colors hover:bg-tint/8 hover:text-foreground focus-visible:ring-2 focus-visible:ring-primary/50"
          >
            Show less
          </button>
        )}
      </header>

      <div
        ref={listRef}
        className={cn('min-h-0 flex-1 px-3 pb-2', expanded ? 'overflow-y-auto' : 'overflow-hidden')}
      >
        {entries.length === 0 ? (
          loading ? (
            <Placeholder withGlyph={Boolean(iconFor) || flags || Boolean(rowIcon)} />
          ) : (
            <p className="py-6 text-center text-xs leading-relaxed text-muted-foreground">
              {emptyHint}
            </p>
          )
        ) : (
          <TooltipProvider delay={250}>
            <ol>
              {shown.map((entry) => (
                <Row
                  key={entry.key}
                  entry={entry}
                  leader={leader}
                  iconFor={iconFor}
                  flag={flags}
                  rowIcon={rowIcon}
                />
              ))}

              {/* The tail the backend summed is its own line once expanded. */}
              {expanded && more && (
                <li>
                  <SummaryLine label={`+${more.count} smaller`} up={more.up} down={more.down} leader={leader} />
                </li>
              )}
            </ol>

            {rest && (
              <button
                type="button"
                onClick={() => setExpanded(true)}
                aria-label={`Show ${rest.count} more`}
                className="group -mx-1 block w-[calc(100%+0.5rem)] rounded-[3px] px-1 text-left outline-none transition-colors hover:bg-tint/5 focus-visible:ring-2 focus-visible:ring-primary/50"
              >
                <SummaryLine label={`+${rest.count} more`} up={rest.up} down={rest.down} leader={leader} interactive />
              </button>
            )}
          </TooltipProvider>
        )}
      </div>
    </motion.section>
  )
}

/** How many fixed-height rows the list's box holds, kept in step with resizes. */
function useRowsThatFit(ref: React.RefObject<HTMLDivElement | null>) {
  const [rows, setRows] = useState(FALLBACK_ROWS)

  useLayoutEffect(() => {
    const element = ref.current
    if (!element) return
    const observer = new ResizeObserver(([entry]) => {
      // The content box: the list's padding is already left out.
      setRows(Math.max(2, Math.floor(entry.contentRect.height / ROW_HEIGHT)))
    })
    observer.observe(element)
    return () => observer.disconnect()
  }, [ref])

  return rows
}

function Row({
  entry,
  leader,
  iconFor,
  flag,
  rowIcon,
}: {
  entry: UsageEntry
  leader: number
  iconFor?: (key: string) => string | undefined
  flag: boolean
  rowIcon?: IconSvgElement
}) {
  const country = flag ? countryName(entry.country) : undefined

  return (
    <Tooltip>
      <TooltipTrigger
        render={<li />}
        className="flex min-w-0 cursor-default flex-col justify-center gap-0.75 outline-none"
        style={{ height: ROW_HEIGHT }}
      >
        <span className="flex min-w-0 items-center gap-2">
          {iconFor && <Glyph src={iconFor(entry.key)} name={entry.key} />}
          {flag && <Flag code={entry.country} />}
          {rowIcon && (
            <HugeiconsIcon icon={rowIcon} size={14} strokeWidth={1.8} className="shrink-0 text-muted-foreground" />
          )}
          <span className="truncate text-xs">{entry.key}</span>
          <span className="numeric ml-auto shrink-0 pl-2 text-xs">{formatBytes(entry.total)}</span>
        </span>
        <SplitBar up={entry.up} down={entry.down} leader={leader} inset={Boolean(iconFor || flag || rowIcon)} />
      </TooltipTrigger>

      <TooltipContent side="top" align="start" className="gap-3">
        <span className="max-w-48 truncate font-medium">{entry.key}</span>
        {country && <span className="opacity-70">{country}</span>}
        <Direction direction="down" bytes={entry.down} />
        <Direction direction="up" bytes={entry.up} />
      </TooltipContent>
    </Tooltip>
  )
}

function Direction({ direction, bytes }: { direction: 'down' | 'up'; bytes: number }) {
  return (
    <span className="numeric flex items-center gap-1 whitespace-nowrap">
      <SeriesIcon direction={direction} size={11} />
      <span className="sr-only">{direction === 'down' ? 'Incoming' : 'Outgoing'}</span>
      {formatBytes(bytes)}
    </span>
  )
}

function SummaryLine({
  label,
  up,
  down,
  leader,
  interactive = false,
}: {
  label: string
  up: number
  down: number
  leader: number
  interactive?: boolean
}) {
  // Spans throughout: this also renders inside a button, which only takes
  // phrasing content.
  return (
    <span className="flex min-w-0 flex-col justify-center gap-0.75" style={{ height: ROW_HEIGHT }}>
      <span className="flex items-center gap-2">
        <HugeiconsIcon icon={GridViewIcon} size={14} strokeWidth={1.8} className="shrink-0 text-muted-foreground" />
        <span
          className={cn(
            'text-xs text-muted-foreground',
            interactive && 'transition-colors group-hover:text-foreground',
          )}
        >
          {label}
        </span>
        <span className="numeric ml-auto text-xs text-muted-foreground">{formatBytes(up + down)}</span>
      </span>
      <SplitBar up={up} down={down} leader={leader} inset muted />
    </span>
  )
}

/**
 * GlassWire's bar: no track, just the fill, starting under the name. It grows
 * in once on mount with a transform (no layout work), then follows refreshes
 * with a width transition.
 */
function SplitBar({
  up,
  down,
  leader,
  inset,
  muted = false,
}: {
  up: number
  down: number
  leader: number
  /** Starts under the name rather than under the row's glyph. */
  inset: boolean
  muted?: boolean
}) {
  const total = up + down
  // A floor keeps a tiny entry visible as a sliver rather than nothing.
  const width = leader > 0 && total > 0 ? Math.max(1.5, Math.min(100, (total / leader) * 100)) : 0

  return (
    <span aria-hidden className={cn('block h-0.75', inset && 'ml-6')}>
      <span
        className={cn(
          'bar-grow flex h-full overflow-hidden transition-[width] duration-500 ease-out',
          muted && 'opacity-50',
        )}
        style={{ width: `${width}%` }}
      >
        {down > 0 && <span className="h-full bg-chart-down" style={{ flexGrow: down }} />}
        {up > 0 && <span className="h-full min-w-px bg-chart-up" style={{ flexGrow: up }} />}
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
      className="grid size-4 shrink-0 place-items-center rounded-[3px] bg-tint/10 text-[9px] font-semibold text-muted-foreground uppercase"
    >
      {name.replace(/[^a-z0-9]/gi, '').charAt(0) || '?'}
    </span>
  )
}

function Placeholder({ withGlyph }: { withGlyph: boolean }) {
  return (
    <div role="status" aria-label="Loading" className="flex flex-col gap-2.5 py-1">
      {[78, 64, 52, 40, 30].map((width) => (
        <div key={width} className="flex flex-col gap-1">
          <div className="flex items-center gap-2">
            {withGlyph && <Skeleton className="size-4 rounded-[3px]" />}
            <Skeleton className="h-2.5 rounded-sm" style={{ width: `${width}%` }} />
          </div>
          <Skeleton className={cn('h-0.75 rounded-none', withGlyph && 'ml-6')} style={{ width: `${width}%` }} />
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
