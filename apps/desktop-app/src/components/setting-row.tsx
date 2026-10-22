/** One labelled control inside a settings group. */

import { createContext, use, type ReactNode } from 'react'
import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/react'
import { motion } from 'motion/react'
import { cn } from 'cn'

import { Switch } from '@/components/ui/switch'
import { staggerChild } from '@/lib/motion'
import { toneStyle, type Tone } from '@/lib/tone'

/**
 * The colour a section's rows put on their icon tiles. Provided once per
 * section rather than passed to every row, so a row moved between sections
 * takes on its new home's colour.
 */
const ToneContext = createContext<Tone | null>(null)

export function SettingTone({ value, children }: { value: Tone; children: ReactNode }) {
  return <ToneContext value={value}>{children}</ToneContext>
}

interface SettingRowProps {
  icon: IconSvgElement
  title: string
  description: string
  /** Rendered on the right; omit when passing `checked`/`onChange`. */
  control?: ReactNode
  checked?: boolean
  onChange?: (next: boolean) => void
  disabled?: boolean
  /** Extra copy shown under the row, e.g. why a control is unavailable. */
  hint?: string
  /** Warnings by default; `muted` for plain status that needs no action. */
  hintTone?: 'warning' | 'muted'
}

export function SettingRow({
  icon,
  title,
  description,
  control,
  checked,
  onChange,
  disabled = false,
  hint,
  hintTone = 'warning',
}: SettingRowProps) {
  const id = `setting-${title.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`
  const tone = use(ToneContext)

  return (
    <motion.div
      variants={staggerChild}
      className={cn('flex flex-col gap-2 px-4 py-3.5', disabled && 'opacity-55')}
    >
      <div className="flex items-center gap-3">
        {tone ? (
          <span
            className="tone-tile grid size-9 shrink-0 place-items-center rounded-xl"
            style={toneStyle(tone)}
          >
            <HugeiconsIcon icon={icon} size={17} strokeWidth={1.8} />
          </span>
        ) : (
          <span className="grid size-9 shrink-0 place-items-center rounded-xl bg-tint/5">
            <HugeiconsIcon icon={icon} size={17} strokeWidth={1.8} className="text-foreground/70" />
          </span>
        )}

        <label htmlFor={id} className="min-w-0 flex-1 cursor-default">
          <span className="block text-sm font-medium">{title}</span>
          <span className="block text-xs leading-relaxed text-muted-foreground">
            {description}
          </span>
        </label>

        {control ?? (
          <Switch
            id={id}
            checked={checked}
            onCheckedChange={onChange}
            disabled={disabled}
            className="shrink-0"
          />
        )}
      </div>

      {hint && (
        <p
          className={cn(
            'pl-12 text-xs',
            hintTone === 'warning' ? 'text-warning' : 'text-muted-foreground',
          )}
        >
          {hint}
        </p>
      )}
    </motion.div>
  )
}

/** Groups rows into one panel with hairline dividers. */
export function SettingGroup({
  title,
  children,
}: {
  title?: string
  children: ReactNode
}) {
  return (
    <section className="flex flex-col gap-2">
      {title && (
        <h2 className="px-1 text-xs font-semibold tracking-wide text-muted-foreground uppercase">
          {title}
        </h2>
      )}
      <div className="panel divide-y divide-tint/6 overflow-hidden rounded-2xl">{children}</div>
    </section>
  )
}
