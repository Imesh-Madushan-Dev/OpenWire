/**
 * Share-link helpers for the server UI.
 *
 * Parsing and validation live in the backend (`core/catalog.rs`); the UI only
 * ever builds a link from what the user typed, then hands it over to be
 * parsed like any pasted one - so there is one importer, not two.
 */

import jsQR from 'jsqr'

import type { ServerSummary, Transport } from '@/lib/types'

export type LinkProtocol = 'vless' | 'trojan'

export const PROTOCOLS: readonly { value: LinkProtocol; label: string; secret: string }[] = [
  { value: 'vless', label: 'VLESS', secret: 'UUID' },
  { value: 'trojan', label: 'Trojan', secret: 'Password' },
]

/** The fake HTTP header is left to share links: nobody types one in. */
export type ManualTransport = Exclude<Transport, 'tcp-http'>

export const TRANSPORTS: readonly { value: ManualTransport; label: string }[] = [
  { value: 'tcp', label: 'TCP' },
  { value: 'ws', label: 'WebSocket' },
  { value: 'grpc', label: 'gRPC' },
  { value: 'xhttp', label: 'XHTTP' },
  { value: 'httpupgrade', label: 'HTTPUpgrade' },
  { value: 'h2', label: 'HTTP/2' },
]

const TRANSPORT_CHIPS: Record<Transport, string> = {
  tcp: 'TCP',
  'tcp-http': 'TCP+HTTP',
  ws: 'WS',
  httpupgrade: 'HTTPUpgrade',
  grpc: 'gRPC',
  h2: 'H2',
  xhttp: 'XHTTP',
}

/** Chips for a server's transport stack, e.g. VLESS · WS · TLS. */
export function protocolChips(server: ServerSummary): string[] {
  if (server.protocol === 'direct') return ['Direct']
  return [
    server.protocol === 'vless' ? 'VLESS' : 'Trojan',
    TRANSPORT_CHIPS[server.transport ?? 'tcp'],
    server.tls ? 'TLS' : 'No TLS',
  ]
}

export interface ManualServer {
  protocol: LinkProtocol
  name: string
  host: string
  port: string
  secret: string
  transport: ManualTransport
  /** The path, or for gRPC the service name. */
  path: string
  /** The HTTP `Host` the transport sends, when it differs from the SNI. */
  hostHeader: string
  tls: boolean
  sni: string
  allowInsecure: boolean
}

/** The share link a server with these details would have. */
export function buildLink(server: ManualServer): string {
  const host = server.host.trim()
  const bracketed = host.includes(':') && !host.startsWith('[') ? `[${host}]` : host
  const params = new URLSearchParams()
  if (server.protocol === 'vless') params.set('encryption', 'none')
  params.set('security', server.tls ? 'tls' : 'none')
  params.set('type', server.transport === 'h2' ? 'http' : server.transport)

  const path = server.path.trim()
  if (server.transport !== 'tcp') {
    if (server.transport === 'grpc') {
      if (path) params.set('serviceName', path)
    } else if (path) {
      params.set('path', path.startsWith('/') ? path : `/${path}`)
    }
    if (server.hostHeader.trim()) params.set('host', server.hostHeader.trim())
  }
  if (server.tls) {
    if (server.sni.trim()) params.set('sni', server.sni.trim())
    if (server.allowInsecure) params.set('allowInsecure', '1')
  }

  const secret = encodeURIComponent(server.secret.trim())
  const label = encodeURIComponent(server.name.trim() || host)
  // Form encoding writes spaces as `+`, which share links read literally; a
  // real `+` is already `%2B`, so every `+` left is a space.
  const query = params.toString().replace(/\+/g, '%20')
  return `${server.protocol}://${secret}@${bracketed}:${server.port.trim()}?${query}#${label}`
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
