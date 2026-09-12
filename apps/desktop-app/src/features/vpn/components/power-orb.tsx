/**
 * The connect control.
 *
 * State is carried by three independent signals - ring colour, glow intensity,
 * and motion - so the current phase is readable at a glance, and still readable
 * with motion disabled or colour vision differences.
 */

import { useEffect, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { PowerIcon } from '@hugeicons/core-free-icons'
import {
  animate,
  AnimatePresence,
  motion,
  useMotionValue,
  useReducedMotion,
  useTransform,
} from 'motion/react'
import { cn } from 'cn'

import { spring, transition } from '@/lib/motion'
import { isBusy, type Phase } from '@/lib/types'

interface PowerOrbProps {
  phase: Phase
  onToggle: () => void
  disabled?: boolean
}

/** Geometry of the progress ring, in the SVG's own 100x100 units. */
const RADIUS = 49
const CIRCUMFERENCE = 2 * Math.PI * RADIUS

/**
 * Where the connecting animation creeps to on its own.
 *
 * Short of 100 on purpose: the handshake takes as long as it takes, and a ring
 * sitting at 100% while still negotiating would be telling the user it had
 * finished. The last stretch is drawn only once the tunnel is actually up.
 */
const CREEP_TARGET = 92
const CREEP_SECONDS = 9

/** How long 100% stays on screen before the orb returns to its icon. */
const HOLD_MS = 900

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

  const progress = useMotionValue(0)
  const percent = useTransform(progress, (value) => `${Math.round(value)}%`)
  const dashOffset = useTransform(
    progress,
    (value) => CIRCUMFERENCE * (1 - Math.min(100, value) / 100),
  )

  // On screen from the moment the user commits until a beat after 100%, so the
  // count is never left frozen mid-climb by a phase that moved on. Adjusted
  // while rendering, on the phase change itself - React's pattern for state
  // that follows a prop - rather than one effect-driven render later.
  const [counting, setCounting] = useState(false)
  const [seenPhase, setSeenPhase] = useState(phase)
  if (phase !== seenPhase) {
    setSeenPhase(phase)
    if (phase === 'connecting') setCounting(true)
    // A failure back to disconnected, or a disconnect, drops the readout rather
    // than leaving a stale number over the icon. `connected` keeps it for the
    // hold below. A session already up at launch never saw `connecting`, so
    // it never shows a count nobody asked for.
    else if (phase !== 'connected') setCounting(false)
  }

  useEffect(() => {
    if (phase === 'connecting') {
      progress.set(0)
      const controls = animate(progress, CREEP_TARGET, {
        duration: reduceMotion ? 0 : CREEP_SECONDS,
        ease: 'easeOut',
      })
      return () => controls.stop()
    }

    if (phase === 'connected') {
      const controls = animate(progress, 100, {
        duration: reduceMotion ? 0 : 0.35,
        ease: 'easeOut',
      })
      const timer = window.setTimeout(() => setCounting(false), HOLD_MS)
      return () => {
        controls.stop()
        window.clearTimeout(timer)
      }
    }

    // Any other phase: the readout is already dropped during render above, so
    // only the ring's value needs resetting for the next attempt.
    progress.set(0)
  }, [phase, progress, reduceMotion])

  return (
    <div className="relative grid place-items-center">
      {/* Ambient halo. Sits behind everything and never intercepts the pointer. */}
      <motion.div
        aria-hidden
        className="pointer-events-none absolute size-85 rounded-full blur-3xl"
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

      {/* Expanding pulse, only once the connect readout has cleared. */}
      <AnimatePresence>
        {phase === 'connected' && !counting && !reduceMotion && (
          <motion.div
            aria-hidden
            key="pulse"
            className="pointer-events-none absolute size-49 rounded-full border border-primary/40"
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
          'group relative grid size-49 place-items-center rounded-full border-2',
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

        {/* Determinate ring, drawn from the same value the number reads. */}
        <AnimatePresence>
          {counting && (
            <motion.svg
              key="progress"
              aria-hidden
              viewBox="0 0 100 100"
              className="absolute inset-0 size-full -rotate-90"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={transition.fast}
            >
              <motion.circle
                cx="50"
                cy="50"
                r={RADIUS}
                fill="none"
                stroke="var(--primary)"
                strokeWidth="2"
                strokeLinecap="round"
                strokeDasharray={CIRCUMFERENCE}
                style={{ strokeDashoffset: dashOffset }}
              />
            </motion.svg>
          )}
        </AnimatePresence>

        {/* Indeterminate sweep while tearing down: there is no progress to show. */}
        <AnimatePresence>
          {phase === 'disconnecting' && (
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
                r={RADIUS}
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

        {/* The count replaces the icon rather than crowding it. */}
        <AnimatePresence mode="wait" initial={false}>
          {counting ? (
            <motion.span
              key="percent"
              initial={{ opacity: 0, scale: 0.9 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 1.05 }}
              transition={transition.fast}
              className="numeric text-4xl font-semibold tabular-nums text-primary"
            >
              <motion.span>{percent}</motion.span>
            </motion.span>
          ) : (
            <motion.span
              key="icon"
              initial={{ opacity: 0, scale: 0.9 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.9 }}
              transition={transition.fast}
              className="grid place-items-center"
            >
              <HugeiconsIcon
                icon={PowerIcon}
                size={56}
                strokeWidth={1.6}
                className={cn('transition-colors duration-300', treatment.icon)}
              />
            </motion.span>
          )}
        </AnimatePresence>
      </motion.button>
    </div>
  )
}
