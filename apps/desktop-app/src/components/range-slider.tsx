/**
 * GlassWire's timeline scrubber: a thin track with two round grey knobs that
 * bracket the part of the overview the main view shows.
 *
 * Built on Base UI's slider for the keyboard and pointer handling - arrow
 * keys nudge a knob, Page Up/Down jump it - with the look owned here rather
 * than in the vendored primitive.
 */

import { Slider as SliderPrimitive } from '@base-ui/react/slider'
import { cn } from 'cn'

interface RangeSliderProps {
  /** Positions run from 0 to `steps`. */
  steps: number
  value: readonly [number, number]
  onValueChange: (value: [number, number]) => void
  /** Once a drag or key press ends - for work too heavy to repeat on every step. */
  onValueCommit?: (value: [number, number]) => void
  /** The narrowest the bracket may get, in steps. */
  minSpan?: number
  /** How far Page Up/Down and Shift+arrows move a knob; arrows move one step. */
  largeStep?: number
  'aria-label': string
  /** Read out for each knob, e.g. the time it sits at; `thumb` is 0 for the start. */
  formatValue?: (position: number, thumb: number) => string
  className?: string
}

export function RangeSlider({
  steps,
  value,
  onValueChange,
  onValueCommit,
  minSpan = 1,
  largeStep,
  'aria-label': ariaLabel,
  formatValue,
  className,
}: RangeSliderProps) {
  return (
    <SliderPrimitive.Root
      value={[value[0], value[1]]}
      min={0}
      max={steps}
      minStepsBetweenValues={minSpan}
      largeStep={largeStep ?? Math.max(1, Math.round(steps / 10))}
      onValueChange={(next) => {
        if (Array.isArray(next) && next.length === 2) onValueChange([next[0], next[1]])
      }}
      onValueCommitted={(next) => {
        if (onValueCommit && Array.isArray(next) && next.length === 2) {
          onValueCommit([next[0], next[1]])
        }
      }}
      // Centred on their positions, so a knob sits exactly over the edge of the
      // bracket it draws - callers inset the track by half a knob to match.
      thumbAlignment="center"
      aria-label={ariaLabel}
      className={cn('w-full', className)}
    >
      <SliderPrimitive.Control className="relative flex h-4 w-full touch-none items-center select-none">
        <SliderPrimitive.Track className="relative h-1 w-full rounded-full bg-tint/12">
          <SliderPrimitive.Indicator className="h-full rounded-full bg-tint/28" />
        </SliderPrimitive.Track>
        {[0, 1].map((index) => (
          <SliderPrimitive.Thumb
            key={index}
            index={index}
            getAriaValueText={formatValue ? (_text, position, thumb) => formatValue(position, thumb) : undefined}
            aria-label={index === 0 ? `${ariaLabel} start` : `${ariaLabel} end`}
            className={cn(
              'block size-4 rounded-full bg-[color-mix(in_oklab,var(--tint)_42%,var(--background))] shadow-sm outline-none',
              'transition-[background-color,box-shadow] duration-150',
              'hover:bg-[color-mix(in_oklab,var(--tint)_55%,var(--background))] focus-visible:ring-2 focus-visible:ring-primary/60',
              'data-dragging:bg-[color-mix(in_oklab,var(--tint)_60%,var(--background))]',
            )}
          />
        ))}
      </SliderPrimitive.Control>
    </SliderPrimitive.Root>
  )
}
