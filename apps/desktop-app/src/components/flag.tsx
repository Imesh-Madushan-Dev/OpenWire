/**
 * A country's flag, from the SVGs in `public/flags`.
 *
 * Images rather than flag emoji: Windows ships no flag glyphs, so emoji
 * render as bare letter pairs. Where the country is unknown - a host reached
 * through the tunnel, whose address only the server saw - a globe holds the
 * same slot, so names stay aligned down the column.
 */

import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { GlobalIcon } from '@hugeicons/core-free-icons'
import { cn } from 'cn'

/** Localised country names, for tooltips: "SG" -> "Singapore". */
const REGION_NAMES = new Intl.DisplayNames(undefined, { type: 'region' })

export function countryName(code: string | undefined): string | undefined {
  if (!code) return undefined
  try {
    return REGION_NAMES.of(code.toUpperCase())
  } catch {
    return undefined
  }
}

export function Flag({ code, className }: { code?: string; className?: string }) {
  // A code the folder has no file for falls back to the globe, once.
  const [missing, setMissing] = useState<string | null>(null)
  const known = code && code !== missing ? code.toUpperCase() : null

  if (!known) {
    return (
      <span aria-hidden className={cn('grid h-[11px] w-4 shrink-0 place-items-center', className)}>
        <HugeiconsIcon icon={GlobalIcon} size={12} strokeWidth={1.8} className="text-muted-foreground" />
      </span>
    )
  }

  return (
    <img
      src={`/flags/${known}.svg`}
      alt=""
      aria-hidden
      loading="lazy"
      decoding="async"
      onError={() => setMissing(code ?? null)}
      // 3:2, like the source art. The hairline ring keeps white flags from
      // dissolving into a light surface.
      className={cn('h-[11px] w-4 shrink-0 rounded-[2px] object-cover ring-1 ring-black/15', className)}
    />
  )
}
