/** Segmented control with a single sliding indicator. */

import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/react'
import { motion } from 'motion/react'
import { cn } from 'cn'

import { spring } from '@/lib/motion'

interface SegmentedProps<T extends string> {
  /** An icon, when given, sits before the label. */
  options: readonly { value: T; label: string; icon?: IconSvgElement }[]
  value: T
  onChange: (value: T) => void
  /** Distinguishes indicators when several controls share a page. */
  layoutId: string
  size?: 'sm' | 'default'
  'aria-label': string
}

export function Segmented<T extends string>({
  options,
  value,
  onChange,
  layoutId,
  size = 'default',
  'aria-label': ariaLabel,
}: SegmentedProps<T>) {
  return (
    <div
      role="tablist"
      aria-label={ariaLabel}
      className="panel inline-flex shrink-0 items-center gap-0.5 rounded-md p-0.5"
    >
      {options.map((option) => {
        const selected = option.value === value
        return (
          <button
            key={option.value}
            type="button"
            role="tab"
            aria-selected={selected}
            onClick={() => onChange(option.value)}
            className={cn(
              'relative rounded-[3px] font-medium whitespace-nowrap transition-colors outline-none',
              'focus-visible:ring-2 focus-visible:ring-primary/50',
              size === 'sm' ? 'px-2.5 py-1 text-[11px]' : 'px-3 py-1.5 text-xs',
              selected ? 'text-foreground' : 'text-muted-foreground hover:text-foreground',
            )}
          >
            {/* A raised neutral chip, as on the toolbars: a view setting, not a call to action. */}
            {selected && (
              <motion.span
                layoutId={layoutId}
                aria-hidden
                className="absolute inset-0 rounded-[3px] bg-tint/14"
                transition={spring.soft}
              />
            )}
            <span className="relative flex items-center gap-1">
              {option.icon && (
                <HugeiconsIcon icon={option.icon} size={size === 'sm' ? 13 : 14} strokeWidth={2} />
              )}
              {option.label}
            </span>
          </button>
        )
      })}
    </div>
  )
}
