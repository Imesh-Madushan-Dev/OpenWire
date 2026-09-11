/** One labelled control inside a settings group. */

import type { ReactNode } from 'react'
import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/react'
import { motion } from 'motion/react'
import { cn } from 'cn'

import { Switch } from '@/components/ui/switch'
import { staggerChild } from '@/lib/motion'

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
}: SettingRowProps) {
  const id = `setting-${title.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`

  return (
    <motion.div
      variants={staggerChild}
      className={cn('flex flex-col gap-2 px-4 py-3.5', disabled && 'opacity-55')}
    >
      <div className="flex items-center gap-3">
        <span className="grid size-9 shrink-0 place-items-center rounded-xl bg-white/5">
          <HugeiconsIcon icon={icon} size={17} strokeWidth={1.8} className="text-primary" />
        </span>

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

      {hint && <p className="pl-12 text-xs text-warning">{hint}</p>}
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
      <div className="panel divide-y divide-white/6 overflow-hidden rounded-2xl">{children}</div>
    </section>
  )
}
