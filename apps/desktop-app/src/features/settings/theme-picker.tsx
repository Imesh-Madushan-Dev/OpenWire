/**
 * Appearance: a colour theme, and whether it is light, dark, or follows the
 * device. The two are independent - every theme has both modes.
 *
 * Each preview carries its own `data-theme` and `data-mode`, so it is painted
 * by that pair's real tokens - the swatches can never drift from what the app
 * will look like. A theme card shows both its modes, split on the diagonal.
 */

import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Tick02Icon } from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'
import { cn } from 'cn'

import { Segmented } from '@/components/segmented'
import { staggerChild } from '@/lib/motion'
import {
  MODES,
  readMode,
  readTheme,
  saveMode,
  saveTheme,
  THEMES,
  type ResolvedMode,
  type ThemeId,
  type ThemeMode,
} from '@/lib/theme'

export function ThemePicker() {
  const [theme, setTheme] = useState<ThemeId>(readTheme)
  const [mode, setMode] = useState<ThemeMode>(readMode)

  const chooseTheme = (next: ThemeId) => {
    setTheme(next)
    saveTheme(next)
  }

  const chooseMode = (next: ThemeMode) => {
    setMode(next)
    saveMode(next)
  }

  return (
    <section className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-3 px-1">
        <h2 className="text-xs font-semibold tracking-wide text-muted-foreground uppercase">Theme</h2>
        <Segmented
          options={MODES}
          value={mode}
          onChange={chooseMode}
          layoutId="appearance-mode"
          size="sm"
          aria-label="Light or dark"
        />
      </div>
      <div role="radiogroup" aria-label="Theme" className="grid grid-cols-3 gap-3">
        {THEMES.map((option) => {
          const selected = option.id === theme
          return (
            <motion.button
              key={option.id}
              variants={staggerChild}
              type="button"
              role="radio"
              aria-checked={selected}
              onClick={() => chooseTheme(option.id)}
              className={cn(
                'panel-interactive flex flex-col gap-2.5 rounded-2xl p-2 text-left outline-none',
                'focus-visible:ring-2 focus-visible:ring-primary/50',
                selected && 'border-primary/60',
              )}
            >
              <span
                aria-hidden
                className="relative block h-22 overflow-hidden rounded-xl border border-hairline"
              >
                <MiniShell theme={option.id} mode="dark" />
                <MiniShell
                  theme={option.id}
                  mode="light"
                  className="[clip-path:polygon(100%_0,100%_100%,0_100%)]"
                />
              </span>
              <span className="flex items-start gap-2 px-1.5 pb-1">
                <span className="min-w-0 flex-1">
                  <span className="block text-sm font-medium">{option.label}</span>
                  <span className="block text-xs leading-relaxed text-muted-foreground">
                    {option.description}
                  </span>
                </span>
                {selected && (
                  <span className="action-fill mt-0.5 grid size-4.5 shrink-0 place-items-center rounded-full bg-primary text-primary-foreground">
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

/** A miniature of the shell: tabbed header, connected orb, and the server column. */
function MiniShell({
  theme,
  mode,
  className,
}: {
  theme: ThemeId
  mode: ResolvedMode
  className?: string
}) {
  return (
    <span
      data-theme={theme}
      data-mode={mode}
      className={cn('shell-canvas absolute inset-0 flex flex-col bg-background', className)}
    >
      <span className="flex h-4 shrink-0 items-center justify-center gap-1.5 border-b border-hairline bg-rail">
        <span className="relative h-1 w-3 rounded-full bg-primary">
          <span className="action-fill absolute inset-x-0.5 -bottom-1 h-px rounded-full bg-primary" />
        </span>
        <span className="h-1 w-3 rounded-full bg-tint/20" />
        <span className="h-1 w-3 rounded-full bg-tint/20" />
        <span className="h-1 w-3 rounded-full bg-tint/20" />
      </span>

      <span className="flex min-h-0 flex-1">
        <span className="relative grid flex-1 place-items-center">
          <span className="ambient-field absolute inset-0" />
          <span className="orb-surface relative size-8 rounded-full">
            <span className="orb-ring absolute inset-0 rounded-full" />
          </span>
        </span>
        <span className="flex w-11 flex-col gap-1 border-l border-hairline bg-rail/50 p-1.5">
          <span className="action-fill h-2 w-5 self-end rounded-full bg-primary" />
          <span className="h-2.5 rounded-sm border border-selected-border bg-selected" />
          <span className="flex h-2.5 items-center gap-0.5 rounded-sm bg-tint/5 px-0.5">
            <span className="size-1.5 rounded-xs bg-(--tone-amber)" />
            <span className="size-1.5 rounded-xs bg-(--tone-violet)" />
          </span>
        </span>
      </span>
    </span>
  )
}
