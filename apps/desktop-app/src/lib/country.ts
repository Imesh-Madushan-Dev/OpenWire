/** Country names for display. Kept apart from `<Flag>` so that file exports only a component. */

/** Localised country names, for tooltips: "SG" -> "Singapore". */
const REGION_NAMES = new Intl.DisplayNames(undefined, { type: 'region' })

export function countryName(code: string | undefined): string | undefined {
  if (!code) return undefined
  try {
    return REGION_NAMES.of(code.toUpperCase())
  } catch {
    return undefined
  }
}
