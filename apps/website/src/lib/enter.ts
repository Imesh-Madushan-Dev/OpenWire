import type { CSSProperties } from 'react';

/** When a first-screen element starts its entrance (globals.css: `.rise` and `[data-enter]`). */
export const delay = (seconds: number) => ({ '--d': `${seconds}s` }) as CSSProperties;
