/**
 * Share-link helpers for the server UI.
 *
 * Parsing and validation live in the backend (`core/catalog.rs`); the UI only
 * ever builds a link from what the user typed, then hands it over to be
 * parsed like any pasted one - so there is one importer, not two.
 */

import jsQR from 'jsqr'

import type { ServerSummary } from '@/lib/types'

export type LinkProtocol = 'vless' | 'trojan'

export const PROTOCOLS: readonly { value: LinkProtocol; label: string; secret: string }[] = [
  { value: 'vless', label: 'VLESS', secret: 'UUID' },
  { value: 'trojan', label: 'Trojan', secret: 'Password' },
]

/** Chips for a server's transport stack, e.g. VLESS · TCP · TLS. */
export function protocolChips(server: ServerSummary): string[] {
  switch (server.protocol) {
    case 'vless-tls':
      return ['VLESS', 'TCP', 'TLS']
    case 'trojan-tls':
      return ['Trojan', 'TCP', 'TLS']
    case 'direct':
      return ['Direct']
  }
}

export interface ManualServer {
  protocol: LinkProtocol
  name: string
  host: string
  port: string
  secret: string
  sni: string
  allowInsecure: boolean
}

/** The share link a server with these details would have. */
export function buildLink(server: ManualServer): string {
  const host = server.host.trim()
  const bracketed = host.includes(':') && !host.startsWith('[') ? `[${host}]` : host
  const params = new URLSearchParams()
  if (server.protocol === 'vless') params.set('encryption', 'none')
  params.set('security', 'tls')
  params.set('type', 'tcp')
  if (server.sni.trim()) params.set('sni', server.sni.trim())
  if (server.allowInsecure) params.set('allowInsecure', '1')

  const secret = encodeURIComponent(server.secret.trim())
  const label = encodeURIComponent(server.name.trim() || host)
  return `${server.protocol}://${secret}@${bracketed}:${server.port.trim()}?${params}#${label}`
}

/** A dropped or picked file's text: a list of links, or a subscription body. */
export async function readTextFile(file: File): Promise<string> {
  // Anything bigger is not a link list.
  if (file.size > 2 * 1024 * 1024) throw new Error('That file is too large to be a server list.')
  return file.text()
}

/**
 * Decodes the QR code in an image file - a screenshot, or a photo of one.
 * Throws with a message for the user when there is none to read.
 */
export async function readQrImage(file: File): Promise<string> {
  const bitmap = await createImageBitmap(file).catch(() => {
    throw new Error("That file isn't an image OpenWire can read.")
  })

  // Big screenshots are scaled down: QR modules survive it, and decoding a
  // 4K frame pixel by pixel takes seconds for nothing.
  const scale = Math.min(1, 1600 / Math.max(bitmap.width, bitmap.height))
  const width = Math.max(1, Math.round(bitmap.width * scale))
  const height = Math.max(1, Math.round(bitmap.height * scale))

  const canvas = new OffscreenCanvas(width, height)
  const context = canvas.getContext('2d', { willReadFrequently: true })
  if (!context) throw new Error('Could not read that image.')
  context.drawImage(bitmap, 0, 0, width, height)
  bitmap.close()

  const pixels = context.getImageData(0, 0, width, height)
  const code = jsQR(pixels.data, width, height, { inversionAttempts: 'attemptBoth' })
  if (!code?.data) throw new Error('No QR code found in that image.')
  return code.data
}
