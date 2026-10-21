/**
 * Theme choice: light, dark, or follow the device.
 *
 * Each preview carries its own `data-theme`, so it is painted by that theme's
 * real tokens - the swatches can never drift from what the app will look like.
 */

import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Tick02Icon } from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'
import { cn } from 'cn'

import { staggerChild } from '@/lib/motion'
import { readTheme, saveTheme, THEMES, type ResolvedTheme, type ThemeMode } from '@/lib/theme'

export function ThemePicker() {
  const [current, setCurrent] = useState<ThemeMode>(readTheme)

  const choose = (mode: ThemeMode) => {
    setCurrent(mode)
    saveTheme(mode)
  }

  return (
    <section className="flex flex-col gap-2">
      <h2 className="px-1 text-xs font-semibold tracking-wide text-muted-foreground uppercase">
        Theme
      </h2>
      <div role="radiogroup" aria-label="Theme" className="grid grid-cols-3 gap-3">
        {THEMES.map((theme) => {
          const selected = theme.id === current
          return (
            <motion.button
              key={theme.id}
              variants={staggerChild}
              type="button"
              role="radio"
              aria-checked={selected}
              onClick={() => choose(theme.id)}
              className={cn(
                'panel-interactive flex flex-col gap-2.5 rounded-2xl p-2 text-left outline-none',
                'focus-visible:ring-2 focus-visible:ring-primary/50',
                selected && 'border-primary/60',
              )}
            >
              <ThemePreview mode={theme.id} />
              <span className="flex items-start gap-2 px-1.5 pb-1">
                <span className="min-w-0 flex-1">
                  <span className="block text-sm font-medium">{theme.label}</span>
                  <span className="block text-xs leading-relaxed text-muted-foreground">
                    {theme.description}
                  </span>
                </span>
                {selected && (
                  <span className="mt-0.5 grid size-4.5 shrink-0 place-items-center rounded-full bg-primary text-primary-foreground">
                    <HugeiconsIcon icon={Tick02Icon} size={11} strokeWidth={2.8} />
                  </span>
                )}
              </span>
            </motion.button>
          )
        })}
      </div>
    </section>
  )
}

/** System shows both themes, split on the diagonal. */
function ThemePreview({ mode }: { mode: ThemeMode }) {
  return (
    <span
      aria-hidden
      className="relative block h-22 overflow-hidden rounded-xl border border-hairline"
    >
      {mode === 'system' ? (
        <>
          <MiniShell theme="dark" />
          <MiniShell
            theme="light"
            className="[clip-path:polygon(100%_0,100%_100%,0_100%)]"
          />
        </>
      ) : (
        <MiniShell theme={mode} />
      )}
    </span>
  )
}

/** A miniature of the shell: rail, ambient glow, orb, and a location card. */
function MiniShell({ theme, className }: { theme: ResolvedTheme; className?: string }) {
  return (
    <span data-theme={theme} className={cn('absolute inset-0 flex bg-background', className)}>
      <span className="flex w-6 shrink-0 flex-col items-center gap-1.5 bg-rail py-2">
        <span className="mb-0.5 size-2.5 rounded-[4px] bg-primary/25" />
        <span className="h-1 w-3 rounded-full bg-primary" />
        <span className="h-1 w-3 rounded-full bg-tint/15" />
        <span className="h-1 w-3 rounded-full bg-tint/15" />
      </span>

      <span className="relative grid flex-1 place-items-center bg-[radial-gradient(circle_at_50%_42%,color-mix(in_oklab,var(--primary)_22%,transparent),transparent_62%)]">
        <span className="orb-surface -mt-3 size-9 rounded-full border-2 border-primary" />
        <span className="panel absolute inset-x-3 bottom-2 flex h-3 items-center gap-1 rounded-md px-1">
          <span className="size-1.5 rounded-full bg-primary" />
          <span className="h-1 w-6 rounded-full bg-tint/20" />
        </span>
      </span>
    </span>
  )
}
