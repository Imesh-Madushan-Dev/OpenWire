/**
 * Identity colours for features and settings sections.
 *
 * Each theme and mode defines its own `--tone-*` shades in `index.css`;
 * `.tone-tile` reads whichever one `--tone` points at. A tone names what a
 * thing is, never how it is doing - health stays with success, warning and
 * destructive.
 */

import type { CSSProperties } from 'react'

export type Tone = 'rose' | 'amber' | 'green' | 'teal' | 'sky' | 'violet'

/** Inline style that points `.tone-tile` (or anything reading `--tone`) at a tone. */
export function toneStyle(tone: Tone): CSSProperties {
  return { '--tone': `var(--tone-${tone})` } as CSSProperties
}
