export const prefersReducedMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches;
export const isTouch = () => matchMedia('(hover: none), (pointer: coarse)').matches;
export const isMobile = () => matchMedia('(max-width: 767px)').matches;

// Shared easing vocabulary. Use these names everywhere; never invent per-component curves.
export const ease = {
  out: 'expo.out', // default entrance: arrive along the line
  inOut: 'expo.inOut', // the ring transition
  soft: 'power3.out', // small UI
  spring: 'back.out(1.6)', // the lock at the end of a connect, used rarely
} as const;
