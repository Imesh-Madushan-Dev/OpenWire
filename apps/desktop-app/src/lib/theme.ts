/**
 * Light, dark, or whatever the device uses.
 *
 * `index.css` defines the same tokens under `[data-theme='dark']` and
 * `[data-theme='light']`; this module picks which one `<html>` carries. The
 * choice is a pure UI preference, so it lives in local storage rather than the
 * Rust settings - which also lets it apply before first paint, with no IPC
 * round trip and no flash of the wrong palette.
 */

export type ThemeMode = 'system' | 'light' | 'dark'

export type ResolvedTheme = Exclude<ThemeMode, 'system'>

export interface ThemeOption {
  id: ThemeMode
  label: string
  description: string
}

export const THEMES: readonly ThemeOption[] = [
  { id: 'system', label: 'System', description: 'Follow the device setting.' },
  { id: 'light', label: 'Light', description: 'Light surfaces for bright rooms.' },
  { id: 'dark', label: 'Dark', description: 'Easy on the eyes at night.' },
]

export const DEFAULT_THEME: ThemeMode = 'system'

const STORAGE_KEY = 'maizy.theme'

const darkQuery = window.matchMedia('(prefers-color-scheme: dark)')

function isThemeMode(value: unknown): value is ThemeMode {
  return THEMES.some((theme) => theme.id === value)
}

/** The saved mode, or the default when storage is empty, stale or blocked. */
export function readTheme(): ThemeMode {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    return isThemeMode(stored) ? stored : DEFAULT_THEME
  } catch {
    return DEFAULT_THEME
  }
}

function resolve(mode: ThemeMode): ResolvedTheme {
  if (mode !== 'system') return mode
  return darkQuery.matches ? 'dark' : 'light'
}

/** Paints `mode` onto the document. Safe to call before React mounts. */
export function applyTheme(mode: ThemeMode) {
  const theme = resolve(mode)
  const root = document.documentElement
  root.dataset.theme = theme
  root.classList.toggle('dark', theme === 'dark')
}

/** Applies and remembers a mode. */
export function saveTheme(mode: ThemeMode) {
  applyTheme(mode)
  try {
    localStorage.setItem(STORAGE_KEY, mode)
  } catch {
    // Storage unavailable: the theme still applies for this session.
  }
}

/** Repaints when the device switches theme, while the app is set to follow it. */
export function followSystemTheme() {
  darkQuery.addEventListener('change', () => {
    if (readTheme() === 'system') applyTheme('system')
  })
}
