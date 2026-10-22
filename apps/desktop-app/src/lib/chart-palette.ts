/**
 * Chart colour palettes.
 *
 * Like the theme, a palette is only tokens: `index.css` defines each one's
 * download/upload pair for both modes under `[data-chart]`, and this module
 * picks which one `<html>` carries. It is stored locally for the same reason
 * the theme is - it applies before first paint.
 */

export type ChartPaletteId = 'classic' | 'signal' | 'glacier' | 'citrus' | 'ember'

export interface ChartPaletteOption {
  id: ChartPaletteId
  label: string
  /** The two colours, download first. */
  description: string
}

export const CHART_PALETTES: readonly ChartPaletteOption[] = [
  { id: 'classic', label: 'Classic', description: 'Gold & magenta' },
  { id: 'signal', label: 'Signal', description: 'Blue & orange' },
  { id: 'glacier', label: 'Glacier', description: 'Sky & coral' },
  { id: 'citrus', label: 'Citrus', description: 'Lime & indigo' },
  { id: 'ember', label: 'Ember', description: 'Orange & violet' },
]

export const DEFAULT_CHART_PALETTE: ChartPaletteId = 'classic'

/**
 * Versioned: v1 was saved while Signal was the default, so most stored values
 * there record the old default rather than a deliberate pick. Starting a new
 * key moves everyone onto Classic once, and from then on the choice sticks.
 */
const STORAGE_KEY = 'maizy.chartPalette.v2'

function isChartPaletteId(value: unknown): value is ChartPaletteId {
  return CHART_PALETTES.some((palette) => palette.id === value)
}

/** The saved palette, or the default when storage is empty, stale or blocked. */
export function readChartPalette(): ChartPaletteId {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    return isChartPaletteId(stored) ? stored : DEFAULT_CHART_PALETTE
  } catch {
    return DEFAULT_CHART_PALETTE
  }
}

/** Paints `id` onto the document. Safe to call before React mounts. */
export function applyChartPalette(id: ChartPaletteId) {
  document.documentElement.dataset.chart = id
}

/** Applies and remembers a palette. */
export function saveChartPalette(id: ChartPaletteId) {
  applyChartPalette(id)
  try {
    localStorage.setItem(STORAGE_KEY, id)
  } catch {
    // Storage unavailable: the palette still applies for this session.
  }
}
