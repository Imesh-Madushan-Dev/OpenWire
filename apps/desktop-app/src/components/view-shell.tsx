/** Scrollable page frame shared by the non-dashboard views. */

import type { ReactNode } from 'react'

interface ViewShellProps {
  title: string
  description?: string
  children?: ReactNode
}

export function ViewShell({ title, description, children }: ViewShellProps) {
  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto flex w-full max-w-140 flex-col gap-7 px-8 pt-4 pb-10">
        <header className="flex flex-col gap-1">
          <h1 className="font-heading text-xl font-semibold">{title}</h1>
          {description && (
            <p className="text-sm leading-relaxed text-muted-foreground">{description}</p>
          )}
        </header>
        {children}
      </div>
    </div>
  )
}
