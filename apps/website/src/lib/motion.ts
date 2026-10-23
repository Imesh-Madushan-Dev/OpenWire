/**
 * Motion tokens, the desktop app's own (apps/desktop-app/src/lib/motion.ts), for the parts of the
 * site whose state lives in React: the power orb and the menu. Scroll and page choreography run
 * on GSAP (src/engine), with the same curve.
 */

import type { Transition, Variants } from 'motion/react';

/** Apple-ish ease-out: fast departure, long gentle settle. */
export const EASE = [0.32, 0.72, 0, 1] as const;

export const DURATION = {
  fast: 0.18,
  base: 0.32,
  slow: 0.56,
} as const;

export const transition = {
  fast: { duration: DURATION.fast, ease: EASE },
  base: { duration: DURATION.base, ease: EASE },
  slow: { duration: DURATION.slow, ease: EASE },
} satisfies Record<string, Transition>;

/** Springs for anything the pointer drives directly. */
export const spring = {
  press: { type: 'spring', stiffness: 520, damping: 30, mass: 0.7 },
  soft: { type: 'spring', stiffness: 260, damping: 26, mass: 0.9 },
} satisfies Record<string, Transition>;

/** Cross-fades a value in place without shifting layout. */
export const swapVariants: Variants = {
  initial: { opacity: 0, y: 6 },
  animate: { opacity: 1, y: 0, transition: transition.base },
  exit: { opacity: 0, y: -6, transition: { duration: 0.14, ease: EASE } },
};
