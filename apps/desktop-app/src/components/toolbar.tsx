/**
 * A page's toolbar: GlassWire's second band, directly under the title bar and
 * in the same raised colour, so the two read as one piece of chrome.
 *
 * Three zones - filters on the left, the page's subject in the centre, the
 * period or actions on the right - with the centre one centred on the window
 * however wide the sides grow.
 */

import type { ReactNode } from 'react'
import { motion } from 'motion/react'
import { cn } from 'cn'

import { spring } from '@/lib/motion'

interface ToolbarProps {
  start?: ReactNode
  center?: ReactNode
  end?: ReactNode
  className?: string
}

export function Toolbar({ start, center, end, className }: ToolbarProps) {
  return (
    <div
      className={cn(
        'grid h-10 shrink-0 grid-cols-[1fr_auto_1fr] items-center gap-3 border-b border-hairline bg-rail px-2.5',
        className,
      )}
    >
      <div className="flex min-w-0 items-center gap-1">{start}</div>
      <div className="flex min-w-0 items-center justify-center gap-1">{center}</div>
      <div className="flex min-w-0 items-center justify-end gap-1">{end}</div>
    </div>
  )
}

interface ToolbarChoiceProps<T extends string> {
  options: readonly { value: T; label: string }[]
  value: T
  onChange: (value: T) => void
  /** Distinguishes indicators when several groups share a page. */
  layoutId: string
  'aria-label': string
  className?: string
}

/**
 * GlassWire's flat toolbar choices - "All · Apps · Traffic", "Month … 5
 * Minutes": plain labels, the chosen one lifted onto a chip. A radio group,
 * since picking one replaces the last; arrow keys move between them.
 */
export function ToolbarChoice<T extends string>({
  options,
  value,
  onChange,
  layoutId,
  'aria-label': ariaLabel,
  className,
}: ToolbarChoiceProps<T>) {
  const index = options.findIndex((option) => option.value === value)

  const move = (step: number) => {
    const next = options[(index + step + options.length) % options.length]
    onChange(next.value)
    // Focus follows the choice, as it does in a native radio group.
    requestAnimationFrame(() => {
      document.getElementById(`${layoutId}-${next.value}`)?.focus()
    })
  }

  return (
    <div role="radiogroup" aria-label={ariaLabel} className={cn('flex items-center gap-0.5', className)}>
      {options.map((option) => {
        const selected = option.value === value
        return (
          <button
            key={option.value}
            id={`${layoutId}-${option.value}`}
            type="button"
            role="radio"
            aria-checked={selected}
            tabIndex={selected ? 0 : -1}
            onClick={() => onChange(option.value)}
            onKeyDown={(event) => {
              if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
                event.preventDefault()
                move(1)
              } else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
                event.preventDefault()
                move(-1)
              }
            }}
            className={cn(
              'relative h-6 rounded-[3px] px-2 text-xs font-medium whitespace-nowrap outline-none',
              'transition-colors duration-150 focus-visible:ring-2 focus-visible:ring-primary/50',
              selected ? 'text-foreground' : 'text-foreground/75 hover:bg-tint/6 hover:text-foreground',
            )}
          >
            {selected && (
              <motion.span
                layoutId={layoutId}
                aria-hidden
                className="absolute inset-0 rounded-[3px] bg-tint/14"
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

/** A flat toolbar button: text, an icon, or both, lit only on hover. */
export function ToolbarButton({
  className,
  ...props
}: React.ComponentProps<'button'>) {
  return (
    <button
      type="button"
      className={cn(
        'flex h-6 shrink-0 items-center gap-1.5 rounded-[3px] px-2 text-xs font-medium text-foreground/85 outline-none',
        'transition-colors duration-150 hover:bg-tint/8 hover:text-foreground',
        'focus-visible:ring-2 focus-visible:ring-primary/50 disabled:pointer-events-none disabled:opacity-50',
        'data-popup-open:bg-tint/12 aria-expanded:bg-tint/12 [&_svg]:shrink-0',
        className,
      )}
      {...props}
    />
  )
}
