/** Display formatting. Kept pure so it is trivially testable. */

const BYTE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB'] as const

/** Compact byte total, e.g. `1.4 GB`. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B'

  const exponent = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    BYTE_UNITS.length - 1,
  )
  const value = bytes / 1024 ** exponent
  // One decimal below 100 keeps the width stable without false precision.
  return `${value < 100 && exponent > 0 ? value.toFixed(1) : Math.round(value)} ${BYTE_UNITS[exponent]}`
}

/** Throughput, e.g. `2.4 MB/s`. */
export function formatSpeed(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`
}

/** Elapsed session time as `H:MM:SS`, or `M:SS` under an hour. */
export function formatDuration(totalSeconds: number): string {
  if (!Number.isFinite(totalSeconds) || totalSeconds < 0) return '0:00'

  const seconds = Math.floor(totalSeconds % 60)
  const minutes = Math.floor((totalSeconds / 60) % 60)
  const hours = Math.floor(totalSeconds / 3600)

  const pad = (n: number) => n.toString().padStart(2, '0')
  return hours > 0
    ? `${hours}:${pad(minutes)}:${pad(seconds)}`
    : `${minutes}:${pad(seconds)}`
}

export function formatLatency(ms: number | null): string {
  return ms === null ? '--' : `${ms} ms`
}

/**
 * Quality bucket for a latency reading, used to colour the indicator.
 * `unknown` is deliberately distinct from `poor` - we do not imply a bad
 * connection just because we have not measured one.
 */
export type Quality = 'good' | 'fair' | 'poor' | 'unknown'

export function latencyQuality(ms: number | null): Quality {
  if (ms === null) return 'unknown'
  if (ms < 80) return 'good'
  if (ms < 200) return 'fair'
  return 'poor'
}

/** Two-letter display code for the flag chip. */
export function regionCode(countryCode: string | null | undefined): string {
  if (!countryCode || countryCode === 'xx') return ''
  return countryCode.slice(0, 2).toUpperCase()
}
