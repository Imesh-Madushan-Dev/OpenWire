/**
 * Share a server as a QR code and a link, for another device or another
 * client. The link is fetched only when this opens - it is the one place the
 * backend hands a credential to the UI, and only because the user asked.
 */

import { useEffect, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Copy01Icon, Tick02Icon } from '@hugeicons/core-free-icons'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Skeleton } from '@/components/ui/skeleton'
import { api, VpnError } from '@/lib/ipc'
import type { ServerSummary } from '@/lib/types'
import { useVpn } from '@/features/vpn/vpn-context'

import { QrCode } from './qr-code'

interface ShareDialogProps {
  server: ServerSummary
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function ShareDialog({ server, open, onOpenChange }: ShareDialogProps) {
  const { notify } = useVpn()
  const [link, setLink] = useState<{ id: string; value: string } | null>(null)
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (!open) return
    let cancelled = false
    api
      .shareLink(server.id)
      .then((value) => {
        if (!cancelled) setLink({ id: server.id, value })
      })
      .catch((error: unknown) => {
        notify({
          level: 'error',
          message: error instanceof VpnError ? error.message : 'Could not build the link.',
        })
        onOpenChange(false)
      })
    return () => {
      cancelled = true
    }
  }, [open, server.id, notify, onOpenChange])

  const value = link?.id === server.id ? link.value : null
  const secret = server.protocol === 'trojan' ? 'password' : 'access key'

  const copy = async () => {
    if (!value) return
    try {
      await navigator.clipboard.writeText(value)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1800)
    } catch {
      notify({ level: 'error', message: 'Could not copy to the clipboard.' })
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        // Drop the link as soon as the dialog closes rather than keeping a
        // credential in memory for no reason.
        if (!next) setLink(null)
        onOpenChange(next)
      }}
    >
      <DialogContent className="grid-cols-[minmax(0,1fr)] gap-5 sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>Share {server.name}</DialogTitle>
          <DialogDescription>
            Scan with OpenWire on another device, or any client that speaks{' '}
            {server.protocol === 'trojan' ? 'Trojan' : 'VLESS'}.
          </DialogDescription>
        </DialogHeader>

        <div className="mx-auto">
          {value ? (
            <QrCode value={value} className="size-60 p-1 shadow-lg" />
          ) : (
            <Skeleton className="size-60 rounded-lg" />
          )}
        </div>

        <div className="flex items-center gap-2 rounded-lg bg-tint/5 py-1.5 pr-1.5 pl-3">
          {/* The secret is masked on screen; Copy still copies the real link. */}
          <code className="min-w-0 flex-1 truncate font-mono text-xs text-muted-foreground">
            {value ? maskSecret(value) : 'Building link…'}
          </code>
          <Button size="sm" variant="secondary" disabled={!value} onClick={() => void copy()}>
            <HugeiconsIcon icon={copied ? Tick02Icon : Copy01Icon} strokeWidth={2} />
            {copied ? 'Copied' : 'Copy'}
          </Button>
        </div>

        <p className="text-xs leading-relaxed text-warning">
          This link contains the server&apos;s {secret}. Anyone who has it can use your server.
        </p>
      </DialogContent>
    </Dialog>
  )
}

/** `vless://2b4f…@host` -> `vless://••••••@host`, for display only. */
function maskSecret(link: string): string {
  const scheme = link.indexOf('://')
  const at = link.lastIndexOf('@', link.indexOf('?') === -1 ? undefined : link.indexOf('?'))
  if (scheme === -1 || at <= scheme) return link
  return `${link.slice(0, scheme + 3)}••••••${link.slice(at)}`
}
