/**
 * The connect control.
 *
 * State is carried by three independent signals - ring colour, glow intensity,
 * and motion - so the current phase is readable at a glance, and still readable
 * with motion disabled or colour vision differences.
 */

import { HugeiconsIcon } from '@hugeicons/react'
import { PowerIcon } from '@hugeicons/core-free-icons'
import { AnimatePresence, motion, useReducedMotion } from 'motion/react'
import { cn } from 'cn'

import { spring, transition } from '@/lib/motion'
import { isBusy, type Phase } from '@/lib/types'

interface PowerOrbProps {
  phase: Phase
  onToggle: () => void
  disabled?: boolean
}

/** Per-phase visual treatment, kept in one table rather than spread through JSX. */
const TREATMENT: Record<
  Phase,
  { ring: string; icon: string; glow: number; label: string }
> = {
  disconnected: {
    ring: 'border-white/12',
    icon: 'text-muted-foreground',
    glow: 0.12,
    label: 'Connect',
  },
  connecting: {
    ring: 'border-primary/40',
    icon: 'text-primary',
    glow: 0.5,
    label: 'Connecting',
  },
  connected: {
    ring: 'border-primary',
    icon: 'text-primary',
    glow: 1,
    label: 'Disconnect',
  },
  reconnecting: {
    ring: 'border-warning/70',
    icon: 'text-warning',
    glow: 0.45,
    label: 'Reconnecting',
  },
  disconnecting: {
    ring: 'border-white/20',
    icon: 'text-muted-foreground',
    glow: 0.2,
    label: 'Disconnecting',
  },
  // Reads as off, because it is off - only the dimmer ring hints that
  // something is still running.
  monitoring: {
    ring: 'border-white/12',
    icon: 'text-muted-foreground',
    glow: 0.12,
    label: 'Connect',
  },
}

export function PowerOrb({ phase, onToggle, disabled = false }: PowerOrbProps) {
  const reduceMotion = useReducedMotion()
  const treatment = TREATMENT[phase]
  const busy = isBusy(phase)
  const inert = disabled || busy

  return (
    <div className="relative grid place-items-center">
      {/* Ambient halo. Sits behind everything and never intercepts the pointer. */}
      <motion.div
        aria-hidden
        className="pointer-events-none absolute size-[340px] rounded-full blur-3xl"
        style={{
          background:
            'radial-gradient(circle, color-mix(in oklab, var(--primary) 55%, transparent), transparent 65%)',
        }}
        animate={{
          opacity: treatment.glow * 0.55,
          scale: phase === 'connected' && !reduceMotion ? [1, 1.08, 1] : 1,
        }}
        transition={
          phase === 'connected' && !reduceMotion
            ? { opacity: transition.slow, scale: { duration: 4, repeat: Infinity, ease: 'easeInOut' } }
            : transition.slow
        }
      />

      {/* Expanding pulse, only while a session is live. */}
      <AnimatePresence>
        {phase === 'connected' && !reduceMotion && (
          <motion.div
            aria-hidden
            key="pulse"
            className="pointer-events-none absolute size-[196px] rounded-full border border-primary/40"
            initial={{ opacity: 0, scale: 1 }}
            animate={{ opacity: [0.5, 0], scale: [1, 1.45] }}
            exit={{ opacity: 0 }}
            transition={{ duration: 2.8, repeat: Infinity, ease: 'easeOut' }}
          />
        )}
      </AnimatePresence>

      <motion.button
        type="button"
        onClick={onToggle}
        disabled={inert}
        aria-label={treatment.label}
        aria-busy={busy}
        aria-pressed={phase === 'connected'}
        className={cn(
          'group relative grid size-[196px] place-items-center rounded-full border-2',
          'bg-[radial-gradient(circle_at_50%_30%,rgba(255,255,255,0.06),rgba(0,0,0,0.35))]',
          'outline-none transition-colors duration-300',
          'focus-visible:ring-4 focus-visible:ring-primary/40',
          inert ? 'cursor-progress' : 'cursor-pointer',
          treatment.ring,
        )}
        whileHover={inert ? undefined : { scale: 1.02 }}
        whileTap={inert ? undefined : { scale: 0.96 }}
        transition={spring.press}
      >
        {/* Inner rim: a second, tighter ring adds depth without another element. */}
        <span
          aria-hidden
          className={cn(
            'absolute inset-3 rounded-full border transition-colors duration-300',
            phase === 'connected' ? 'border-primary/25' : 'border-white/5',
          )}
        />

        {/* Indeterminate arc while the tunnel is coming up or down. */}
        <AnimatePresence>
          {busy && (
            <motion.svg
              key="arc"
              aria-hidden
              viewBox="0 0 100 100"
              className="absolute inset-0 size-full"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1, rotate: reduceMotion ? 0 : 360 }}
              exit={{ opacity: 0 }}
              transition={{
                opacity: transition.fast,
                rotate: { duration: 1.1, repeat: Infinity, ease: 'linear' },
              }}
            >
              <circle
                cx="50"
                cy="50"
                r="49"
                fill="none"
                stroke="var(--primary)"
                strokeWidth="2"
                strokeLinecap="round"
                // A quarter-turn sweep of the 2*pi*r circumference.
                strokeDasharray="77 231"
              />
            </motion.svg>
          )}
        </AnimatePresence>

        <HugeiconsIcon
          icon={PowerIcon}
          size={56}
          strokeWidth={1.6}
          className={cn('transition-colors duration-300', treatment.icon)}
        />
      </motion.button>
    </div>
  )
}
