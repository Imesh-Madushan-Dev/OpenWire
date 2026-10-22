/**
 * Country marker: the country's flag, from the same SVGs the Usage hosts use.
 *
 * Deliberately not a flag emoji: Windows ships no flag glyphs, so those render
 * as bare letter pairs and look broken. A server with no country - one added
 * from a link - gets a globe in the same slot.
 */

import { HugeiconsIcon } from '@hugeicons/react'
import { GlobalIcon } from '@hugeicons/core-free-icons'
import { cn } from 'cn'

import { Flag } from '@/components/flag'
import { regionCode } from '@/lib/format'

interface RegionChipProps {
  countryCode: string | null | undefined
  className?: string
  size?: 'sm' | 'md'
}

export function RegionChip({ countryCode, className, size = 'md' }: RegionChipProps) {
  const code = regionCode(countryCode)

  if (code) {
    return (
      <Flag
        code={code}
        className={cn(size === 'sm' ? 'h-3 w-4.5' : 'h-3.5 w-5', className)}
      />
    )
  }

  return (
    <span
      aria-hidden
      className={cn(
        'inline-grid shrink-0 place-items-center rounded-md border border-tint/10',
        'chip-surface',
        size === 'sm' ? 'h-4 w-6' : 'h-5 w-7',
        className,
      )}
    >
      <HugeiconsIcon
        icon={GlobalIcon}
        size={size === 'sm' ? 10 : 12}
        strokeWidth={2}
        className="text-muted-foreground"
      />
    </span>
  )
}
