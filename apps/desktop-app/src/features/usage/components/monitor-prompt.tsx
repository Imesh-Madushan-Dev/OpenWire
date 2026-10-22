/**
 * Stands in for the Hosts and Traffic-type columns when the network monitor
 * has nothing for the period - and says why, with the one action that helps.
 */

import { HugeiconsIcon } from '@hugeicons/react'
import { Alert02Icon, Radar01Icon } from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'
import { cn } from 'cn'

import { Button } from '@/components/ui/button'
import { staggerChild } from '@/lib/motion'

import { monitorSummary, type MonitorState } from '../use-monitor'

interface MonitorPromptProps {
  state: MonitorState
  error: string | null
  busy: boolean
  onEnable: () => void
  className?: string
}

export function MonitorPrompt({ state, error, busy, onEnable, className }: MonitorPromptProps) {
  const copy = COPY[state]
  const troubled = state === 'failing' || state === 'stopped' || state === 'outdated'

  return (
    <motion.section
      variants={staggerChild}
      className={cn('relative isolate overflow-hidden', className)}
    >
      {/* A warning wash only when the monitor needs attention - that is state.
          Otherwise the card stays neutral and the button carries the action. */}
      {troubled && (
        <div
          aria-hidden
          className="absolute inset-0 -z-10 bg-[radial-gradient(120%_80%_at_50%_0%,color-mix(in_oklab,var(--warning)_10%,transparent),transparent_70%)]"
        />
      )}

      <div className="flex h-full flex-col items-center justify-center gap-2.5 px-8 py-6 text-center">
        <span
          className={cn(
            'relative grid size-10 place-items-center rounded-lg',
            troubled ? 'bg-warning/12 text-warning' : 'bg-tint/7 text-foreground/80',
          )}
        >
          {state === 'starting' && (
            <span
              aria-hidden
              className="absolute inset-0 animate-ping rounded-lg bg-tint/15"
            />
          )}
          <HugeiconsIcon icon={troubled ? Alert02Icon : Radar01Icon} size={20} strokeWidth={1.8} />
        </span>

        <h3 className="text-sm font-semibold">{copy.title}</h3>
        <p className="max-w-md text-xs leading-relaxed text-muted-foreground">
          {copy.body ?? monitorSummary(state, error)}
        </p>

        {copy.action && (
          <Button
            size="sm"
            variant={troubled ? 'outline' : 'default'}
            className="mt-1"
            disabled={busy}
            onClick={onEnable}
          >
            {busy ? 'Waiting for Windows…' : copy.action}
          </Button>
        )}
        {copy.action && !troubled && (
          <p className="text-[11px] text-muted-foreground/70">
            Windows asks for admin approval once.
          </p>
        )}
      </div>
    </motion.section>
  )
}

const OFF = {
  title: 'See where all your traffic goes',
  body: "Maizy names hosts and traffic types only for what passes through it. Detailed monitoring reads Windows' own network events to cover every app on this device - no driver, and never the contents of your traffic.",
  action: 'Turn on detailed monitoring',
}

const COPY: Record<MonitorState, { title: string; body?: string; action?: string }> = {
  loading: OFF,
  off: OFF,
  unsupported: {
    title: 'Hosts and traffic types need Windows',
    body: 'Detailed monitoring reads Windows network events, which this platform does not have.',
  },
  starting: {
    title: 'Monitoring is starting',
    body: 'Hosts and traffic types appear here as soon as the first traffic is recorded.',
  },
  active: {
    title: 'Nothing recorded for this period yet',
    body: "Detailed monitoring is on. Periods from before it started only have Windows' per-app totals, shown on the left.",
  },
  failing: { title: "The monitor can't record right now", action: 'Repair' },
  stopped: { title: 'The monitor needs a repair', action: 'Repair' },
  outdated: { title: 'The monitor needs an update', action: 'Update' },
}
