/**
 * Shared motion tokens.
 *
 * One easing curve and three durations across the whole app - motion that is
 * consistent reads as physical; motion that varies per component reads as bugs.
 */

import type { Transition, Variants } from 'motion/react'

/** Apple-ish ease-out: fast departure, long gentle settle. */
export const EASE = [0.32, 0.72, 0, 1] as const

export const DURATION = {
  fast: 0.18,
  base: 0.32,
  slow: 0.56,
} as const

export const transition = {
  fast: { duration: DURATION.fast, ease: EASE },
  base: { duration: DURATION.base, ease: EASE },
  slow: { duration: DURATION.slow, ease: EASE },
} satisfies Record<string, Transition>

/** Springs for anything the pointer drives directly. */
export const spring = {
  press: { type: 'spring', stiffness: 520, damping: 30, mass: 0.7 },
  soft: { type: 'spring', stiffness: 260, damping: 26, mass: 0.9 },
} satisfies Record<string, Transition>

/** Screen-level enter/exit. */
export const viewVariants: Variants = {
  initial: { opacity: 0, y: 8 },
  animate: { opacity: 1, y: 0, transition: transition.base },
  exit: { opacity: 0, y: -6, transition: transition.fast },
}

/** Container that reveals its children in sequence. */
export const staggerParent: Variants = {
  initial: {},
  animate: { transition: { staggerChildren: 0.05, delayChildren: 0.04 } },
}

export const staggerChild: Variants = {
  initial: { opacity: 0, y: 12 },
  animate: { opacity: 1, y: 0, transition: transition.base },
}

/** Cross-fades a value in place without shifting layout. */
export const swapVariants: Variants = {
  initial: { opacity: 0, y: 4 },
  animate: { opacity: 1, y: 0, transition: transition.fast },
  exit: { opacity: 0, y: -4, transition: { duration: 0.12, ease: EASE } },
}
