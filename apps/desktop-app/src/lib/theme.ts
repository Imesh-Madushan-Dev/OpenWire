/**
 * Colour theme and light/dark mode, chosen independently.
 *
 * `index.css` defines every token once per theme and mode, under
 * `[data-theme='…'][data-mode='…']`; this module picks which pair `<html>`
 * carries. Both are pure UI preferences, so they live in local storage rather
 * than the Rust settings - which also lets them apply before first paint,
 * with no IPC round trip and no flash of the wrong palette. The only IPC is
 * after the fact: the window and tray icons follow the mode.
 */

import { api } from './ipc'

export type ThemeId = 'graphite' | 'aurora' | 'neon'

export type ThemeMode = 'system' | 'light' | 'dark'

export type ResolvedMode = Exclude<ThemeMode, 'system'>

export interface ThemeOption {
  id: ThemeId
  label: string
  description: string
}

export const THEMES: readonly ThemeOption[] = [
  { id: 'graphite', label: 'Graphite', description: 'Calm charcoal, colour where it counts.' },
  { id: 'aurora', label: 'Aurora', description: 'Deep indigo and vivid gradients.' },
  { id: 'neon', label: 'Neon', description: 'Near-black with glowing cyan and magenta.' },
]

export const MODES: readonly { value: ThemeMode; label: string }[] = [
  { value: 'system', label: 'System' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
]

export const DEFAULT_THEME: ThemeId = 'graphite'
export const DEFAULT_MODE: ThemeMode = 'system'

const THEME_KEY = 'openwire.colorTheme'
// Named before colour themes existed; it has always held the mode.
const MODE_KEY = 'openwire.theme'

const darkQuery = window.matchMedia('(prefers-color-scheme: dark)')

function read<T extends string>(key: string, valid: readonly T[], fallback: T): T {
  try {
    const stored = localStorage.getItem(key)
    return valid.includes(stored as T) ? (stored as T) : fallback
  } catch {
    return fallback
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value)
  } catch {
    // Storage unavailable: the choice still applies for this session.
  }
}

/** The saved theme, or the default when storage is empty, stale or blocked. */
export function readTheme(): ThemeId {
  return read(
    THEME_KEY,
    THEMES.map((theme) => theme.id),
    DEFAULT_THEME,
  )
}

/** The saved mode, or the default when storage is empty, stale or blocked. */
export function readMode(): ThemeMode {
  return read(
    MODE_KEY,
    MODES.map((mode) => mode.value),
    DEFAULT_MODE,
  )
}

export function resolveMode(mode: ThemeMode): ResolvedMode {
  if (mode !== 'system') return mode
  return darkQuery.matches ? 'dark' : 'light'
}

/** Paints a theme and mode onto the document. Safe to call before React mounts. */
export function applyAppearance(theme: ThemeId, mode: ThemeMode) {
  const resolved = resolveMode(mode)
  const root = document.documentElement
  root.dataset.theme = theme
  root.dataset.mode = resolved
  // shadcn's `dark:` variants key off this class.
  root.classList.toggle('dark', resolved === 'dark')
  // Cosmetic: a failed swap leaves the previous icon, which still works.
  api.setAppIcon(resolved === 'dark').catch(() => {})
}

/** Applies and remembers a theme, keeping the current mode. */
export function saveTheme(theme: ThemeId) {
  applyAppearance(theme, readMode())
  write(THEME_KEY, theme)
}

/** Applies and remembers a mode, keeping the current theme. */
export function saveMode(mode: ThemeMode) {
  applyAppearance(readTheme(), mode)
  write(MODE_KEY, mode)
}

/** Repaints when the device switches between light and dark, while the app follows it. */
export function followSystemMode() {
  darkQuery.addEventListener('change', () => {
    if (readMode() === 'system') applyAppearance(readTheme(), 'system')
  })
}
