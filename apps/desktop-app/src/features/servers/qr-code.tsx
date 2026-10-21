/**
 * A QR code as one SVG path.
 *
 * Always dark modules on a white tile whatever the theme: scanners expect
 * that contrast, and an inverted or tinted code fails on many phone cameras.
 */

import { useMemo } from 'react'
import qrcode from 'qrcode-generator'
import { cn } from 'cn'

export function QrCode({ value, className }: { value: string; className?: string }) {
  const { size, path } = useMemo(() => {
    // Automatic version, medium error correction: room for a long share link
    // while still surviving a slightly blurry photo.
    const qr = qrcode(0, 'M')
    qr.addData(value)
    qr.make()

    const count = qr.getModuleCount()
    let d = ''
    for (let row = 0; row < count; row++) {
      for (let col = 0; col < count; col++) {
        if (qr.isDark(row, col)) d += `M${col} ${row}h1v1h-1z`
      }
    }
    return { size: count, path: d }
  }, [value])

  // The quiet zone the spec asks for, in modules.
  const margin = 2
  return (
    <svg
      role="img"
      aria-label="QR code for this server's link"
      viewBox={`${-margin} ${-margin} ${size + margin * 2} ${size + margin * 2}`}
      shapeRendering="crispEdges"
      className={cn('rounded-2xl bg-white', className)}
    >
      <path d={path} fill="#0b0f0d" />
    </svg>
  )
}
