/** Segmented control with a single sliding indicator. */

import { motion } from 'motion/react'
import { cn } from 'cn'

import { spring } from '@/lib/motion'

interface SegmentedProps<T extends string> {
  options: readonly { value: T; label: string }[]
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
      className="panel inline-flex shrink-0 items-center gap-0.5 rounded-full p-0.5"
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
              'relative rounded-full font-medium whitespace-nowrap transition-colors outline-none',
              'focus-visible:ring-2 focus-visible:ring-primary/50',
              size === 'sm' ? 'px-2.5 py-1 text-[11px]' : 'px-3 py-1.5 text-xs',
              selected ? 'text-foreground' : 'text-muted-foreground hover:text-foreground',
            )}
          >
            {/* A raised neutral chip: choosing a range is a view setting, not a call to action. */}
            {selected && (
              <motion.span
                layoutId={layoutId}
                aria-hidden
                className="absolute inset-0 rounded-full bg-tint/12 shadow-sm"
                transition={spring.soft}
              />
            )}
            <span className="relative">{option.label}</span>
          </button>
        )
      })}
    </div>
  )
}
