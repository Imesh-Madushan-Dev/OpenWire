/**
 * Colour themes.
 *
 * A theme is only a palette: `index.css` defines the same tokens under each
 * `[data-theme]`, and this module picks which one `<html>` carries. The choice
 * is a pure UI preference, so it lives in local storage rather than the Rust
 * settings - which also lets it apply before first paint, with no IPC round
 * trip and no flash of the default theme.
 */

export type ThemeId = 'maizy' | 'ocean' | 'nebula' | 'graphite' | 'daylight'

export interface ThemeOption {
  id: ThemeId
  label: string
  description: string
  mode: 'dark' | 'light'
}

export const THEMES: readonly ThemeOption[] = [
  { id: 'maizy', label: 'Maizy', description: 'The original green on black.', mode: 'dark' },
  { id: 'ocean', label: 'Ocean', description: 'Sky blue on deep navy.', mode: 'dark' },
  { id: 'nebula', label: 'Nebula', description: 'Soft violet on plum.', mode: 'dark' },
  { id: 'graphite', label: 'Graphite', description: 'Monochrome, no hue at all.', mode: 'dark' },
  { id: 'daylight', label: 'Daylight', description: 'Light surfaces for bright rooms.', mode: 'light' },
]

export const DEFAULT_THEME: ThemeId = 'maizy'

const STORAGE_KEY = 'maizy.theme'

function isThemeId(value: unknown): value is ThemeId {
  return THEMES.some((theme) => theme.id === value)
}

/** The saved theme, or the default when storage is empty, stale or blocked. */
export function readTheme(): ThemeId {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    return isThemeId(stored) ? stored : DEFAULT_THEME
  } catch {
    return DEFAULT_THEME
  }
}

/** Paints `id` onto the document. Safe to call before React mounts. */
export function applyTheme(id: ThemeId) {
  const theme = THEMES.find((option) => option.id === id) ?? THEMES[0]
  const root = document.documentElement
  root.dataset.theme = theme.id
  root.classList.toggle('dark', theme.mode === 'dark')
}

/** Applies and remembers a theme. */
export function saveTheme(id: ThemeId) {
  applyTheme(id)
  try {
    localStorage.setItem(STORAGE_KEY, id)
  } catch {
    // Storage unavailable: the theme still applies for this session.
  }
}
