/**
 * Country marker.
 *
 * Deliberately not a flag emoji: Windows ships no flag glyphs, so those render
 * as bare letter pairs and look broken. A typed chip renders identically on
 * every platform.
 */

import { HugeiconsIcon } from '@hugeicons/react'
import { GlobalIcon } from '@hugeicons/core-free-icons'
import { cn } from 'cn'

import { regionCode } from '@/lib/format'

interface RegionChipProps {
  countryCode: string | null | undefined
  className?: string
  size?: 'sm' | 'md'
}

export function RegionChip({ countryCode, className, size = 'md' }: RegionChipProps) {
  const code = regionCode(countryCode)

  return (
    <span
      aria-hidden
      className={cn(
        'inline-grid shrink-0 place-items-center rounded-md border border-tint/10',
        'chip-surface',
        'font-semibold tracking-wider text-foreground/80',
        size === 'sm' ? 'h-4 w-6 text-[9px]' : 'h-5 w-7 text-[10px]',
        className,
      )}
    >
      {code || (
        <HugeiconsIcon
          icon={GlobalIcon}
          size={size === 'sm' ? 10 : 12}
          strokeWidth={2}
          className="text-muted-foreground"
        />
      )}
    </span>
  )
}
