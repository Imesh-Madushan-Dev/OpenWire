'use client';

/**
 * The connect control, ported from the app (features/vpn/components/power-orb.tsx).
 *
 * State is carried by three independent signals - ring colour, glow, and motion - so the phase
 * reads at a glance, and still reads with motion off or with colour vision differences. Here it
 * drives a demo tunnel (src/motion/tunnel.ts) that lights the page instead of a real one.
 */

import { useEffect, useState, useSyncExternalStore } from 'react';
import { HugeiconsIcon } from '@hugeicons/react';
import { PowerIcon } from '@hugeicons/core-free-icons';
import { gsap } from 'gsap';
import { AnimatePresence, useMotionValue, useTransform } from 'motion/react';
import * as m from 'motion/react-m';

import { spring, transition } from '@/lib/motion';
import { useReducedMotion } from '@/lib/use-reduced-motion';
import { CONNECT_S, getPhase, getServerPhase, isBusy, subscribe, toggle, type Phase } from '@/motion/tunnel';

/** Geometry of the progress ring, in the SVG's own 100x100 units. */
const RADIUS = 49;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

/** Where the count creeps to on its own: 100 is only drawn once the tunnel is up. */
const CREEP_TARGET = 92;
/** How long 100% stays on screen before the orb returns to its icon. */
const HOLD_MS = 700;

const GLOW: Record<Phase, number> = { disconnected: 0.4, connecting: 0.45, connected: 0.85, disconnecting: 0.1 };
const ACTION: Record<Phase, string> = {
  disconnected: 'Turn the tunnel on',
  connecting: 'Opening the tunnel',
  connected: 'Turn the tunnel off',
  disconnecting: 'Closing the tunnel',
};

export function useTunnelPhase(pinned?: Phase) {
  const live = useSyncExternalStore(subscribe, getPhase, getServerPhase);
  return pinned ?? live;
}

/**
 * `pinned` holds the orb in one phase (the 404's "off"); `onPress` with `pressLabel` gives a
 * pinned orb its own action, such as reconnecting by going home.
 */
export function PowerOrb({
  pinned,
  onPress,
  pressLabel,
}: {
  pinned?: Phase;
  onPress?: () => void;
  pressLabel?: string;
}) {
  const phase = useTunnelPhase(pinned);
  const reduceMotion = useReducedMotion();
  const busy = isBusy(phase);
  const action = onPress ?? (pinned ? undefined : toggle);
  const interactive = Boolean(action);

  const progress = useMotionValue(0);
  const percent = useTransform(progress, (value) => `${Math.round(value)}%`);
  const dashOffset = useTransform(progress, (value) => CIRCUMFERENCE * (1 - Math.min(100, value) / 100));

  // On screen from the moment the tunnel starts opening until a beat after 100%. Adjusted while
  // rendering, on the phase change itself, as the app does.
  const [counting, setCounting] = useState(false);
  const [seenPhase, setSeenPhase] = useState(phase);
  if (phase !== seenPhase) {
    setSeenPhase(phase);
    if (phase === 'connecting') setCounting(true);
    else if (phase !== 'connected') setCounting(false);
  }

  // The count runs on GSAP's ticker, the site's one clock, and writes into the motion value.
  useEffect(() => {
    const count = (to: number, duration: number) => {
      const state = { value: progress.get() };
      return gsap.to(state, {
        value: to,
        duration: reduceMotion ? 0 : duration,
        ease: 'power2.out',
        onUpdate: () => progress.set(state.value),
      });
    };
    if (phase === 'connecting') {
      progress.set(0);
      const tween = count(CREEP_TARGET, CONNECT_S);
      return () => {
        tween.kill();
      };
    }
    if (phase === 'connected') {
      const tween = count(100, 0.35);
      const timer = window.setTimeout(() => setCounting(false), HOLD_MS);
      return () => {
        tween.kill();
        window.clearTimeout(timer);
      };
    }
    progress.set(0);
  }, [phase, progress, reduceMotion]);

  return (
    <div className="orb" data-phase={phase}>
      <m.div
        aria-hidden
        className="orb__halo"
        initial={false}
        animate={{
          opacity: GLOW[phase] * 0.6,
          scale: phase === 'connected' && !reduceMotion ? [1, 1.08, 1] : 1,
        }}
        transition={
          phase === 'connected' && !reduceMotion
            ? { opacity: transition.slow, scale: { duration: 4, repeat: Infinity, ease: 'easeInOut' } }
            : transition.slow
        }
      />

      <AnimatePresence>
        {phase === 'connected' && !counting && !reduceMotion && (
          <m.div
            aria-hidden
            key="pulse"
            className="orb__pulse"
            initial={{ opacity: 0, scale: 1 }}
            animate={{ opacity: [0.55, 0], scale: [1, 1.45] }}
            exit={{ opacity: 0 }}
            transition={{ duration: 2.8, repeat: Infinity, ease: 'easeOut' }}
          />
        )}
      </AnimatePresence>

      <m.button
        type="button"
        className="orb__button"
        onClick={action}
        tabIndex={interactive ? undefined : -1}
        aria-label={pressLabel ?? ACTION[phase]}
        aria-pressed={pinned ? undefined : phase === 'connected'}
        aria-disabled={busy || !interactive}
        aria-busy={busy}
        whileHover={busy || !interactive ? undefined : { scale: 1.03 }}
        whileTap={busy || !interactive ? undefined : { scale: 0.95 }}
        transition={spring.press}
      >
        <AnimatePresence>
          {phase === 'connected' && (
            <m.span
              key="rim"
              aria-hidden
              className="orb__rim"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={transition.slow}
            />
          )}
        </AnimatePresence>

        <span aria-hidden className="orb__inner" />

        <AnimatePresence>
          {counting && (
            <m.svg
              key="progress"
              aria-hidden
              viewBox="0 0 100 100"
              className="orb__svg"
              style={{ rotate: -90 }}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={transition.fast}
            >
              <m.circle
                cx="50"
                cy="50"
                r={RADIUS}
                fill="none"
                stroke={phase === 'connected' ? 'var(--on)' : 'var(--indigo)'}
                strokeWidth="2"
                strokeLinecap="round"
                strokeDasharray={CIRCUMFERENCE}
                style={{ strokeDashoffset: dashOffset }}
              />
            </m.svg>
          )}
        </AnimatePresence>

        <AnimatePresence>
          {phase === 'disconnecting' && (
            <m.svg
              key="arc"
              aria-hidden
              viewBox="0 0 100 100"
              className="orb__svg"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1, rotate: reduceMotion ? 0 : 360 }}
              exit={{ opacity: 0 }}
              transition={{ opacity: transition.fast, rotate: { duration: 1.1, repeat: Infinity, ease: 'linear' } }}
            >
              <circle
                cx="50"
                cy="50"
                r={RADIUS}
                fill="none"
                stroke="var(--muted)"
                strokeWidth="2"
                strokeLinecap="round"
                strokeDasharray="77 231"
              />
            </m.svg>
          )}
        </AnimatePresence>

        <AnimatePresence mode="wait" initial={false}>
          {counting ? (
            <m.span
              key="percent"
              aria-hidden
              className="orb__count"
              initial={{ opacity: 0, scale: 0.9 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 1.05 }}
              transition={transition.fast}
            >
              <m.span>{percent}</m.span>
            </m.span>
          ) : (
            <m.span
              key="icon"
              aria-hidden
              className="orb__icon"
              initial={{ opacity: 0, scale: 0.9 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.9 }}
              transition={transition.fast}
            >
              <HugeiconsIcon icon={PowerIcon} size={64} strokeWidth={1.6} />
            </m.span>
          )}
        </AnimatePresence>
      </m.button>
    </div>
  );
}
