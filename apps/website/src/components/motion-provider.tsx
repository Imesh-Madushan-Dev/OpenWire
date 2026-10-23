'use client';

import type { ReactNode } from 'react';
import { domAnimation, LazyMotion } from 'motion/react';

/**
 * Motion with only the features the site uses (animate, exit, hover and tap), so the orb and the
 * menu cost a fraction of the full bundle. `strict` makes a stray `motion.div` an error: use `m.*`.
 */
export function MotionProvider({ children }: { children: ReactNode }) {
  return (
    <LazyMotion features={domAnimation} strict>
      {children}
    </LazyMotion>
  );
}
