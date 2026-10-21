/** A labelled control with an optional hint, for the server dialogs. */

import { cloneElement, useId, type ReactElement, type ReactNode } from 'react'
import { cn } from 'cn'

interface FormFieldProps {
  label: string
  hint?: ReactNode
  /** The control; it receives the generated id so the label targets it. */
  children: ReactElement<{ id?: string }>
  className?: string
}

export function FormField({ label, hint, children, className }: FormFieldProps) {
  const id = useId()
  return (
    <div className={cn('flex flex-col gap-1.5', className)}>
      <label htmlFor={id} className="text-xs font-medium text-foreground/85">
        {label}
      </label>
      {cloneElement(children, { id })}
      {hint && <p className="text-[11px] leading-snug text-muted-foreground">{hint}</p>}
    </div>
  )
}

/** A switch row inside a dialog form. */
export function ToggleField({
  label,
  description,
  control,
  tone = 'default',
}: {
  label: string
  description: string
  control: ReactNode
  tone?: 'default' | 'warning'
}) {
  return (
    <div className="flex items-start gap-3 rounded-2xl bg-tint/4 px-3 py-2.5">
      <div className="min-w-0 flex-1">
        <p className="text-xs font-medium">{label}</p>
        <p
          className={cn(
            'text-[11px] leading-snug',
            tone === 'warning' ? 'text-warning' : 'text-muted-foreground',
          )}
        >
          {description}
        </p>
      </div>
      {control}
    </div>
  )
}
