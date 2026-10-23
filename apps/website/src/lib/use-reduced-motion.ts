'use client';

import { useSyncExternalStore } from 'react';

const QUERY = '(prefers-reduced-motion: reduce)';

const subscribe = (onChange: () => void) => {
  const media = matchMedia(QUERY);
  media.addEventListener('change', onChange);
  return () => media.removeEventListener('change', onChange);
};

/**
 * The visitor's reduced-motion setting, safe to read while hydrating: the server and the first
 * client render both say "no", then it updates. (Motion's own hook reads the media query during
 * the first client render, which renders different markup than the server sent.)
 */
export function useReducedMotion() {
  return useSyncExternalStore(
    subscribe,
    () => matchMedia(QUERY).matches,
    () => false,
  );
}
