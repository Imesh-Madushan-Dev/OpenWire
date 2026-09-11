/**
 * Adds a node from a `vless://` share link - the format every VLESS server
 * emits, so this is the one path a user actually has to hand.
 *
 * The link contains an access key, so the field never echoes it back into
 * state once submitted and is typed as a password input.
 */

import { useState, type FormEvent } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Link01Icon, PlusSignIcon } from '@hugeicons/core-free-icons'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useVpn } from '@/features/vpn/vpn-context'

export function ServerImportField({ onAdded }: { onAdded?: () => void }) {
  const { importServer } = useVpn()
  const [uri, setUri] = useState('')
  const [submitting, setSubmitting] = useState(false)

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault()
    const trimmed = uri.trim()
    if (!trimmed || submitting) return

    setSubmitting(true)
    const added = await importServer(trimmed)
    setSubmitting(false)

    if (added) {
      // Clear on success only: a rejected link stays put so it can be fixed.
      setUri('')
      onAdded?.()
    }
  }

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <HugeiconsIcon
            icon={Link01Icon}
            size={15}
            strokeWidth={2}
            className="pointer-events-none absolute top-1/2 left-3.5 -translate-y-1/2 text-muted-foreground"
          />
          <Input
            type="password"
            value={uri}
            onChange={(event) => setUri(event.target.value)}
            placeholder="vless://key@host:443?security=tls&sni=..."
            autoComplete="off"
            spellCheck={false}
            aria-label="Server share link"
            className="h-10 pl-9 font-mono text-xs"
          />
        </div>
        <Button type="submit" size="lg" disabled={!uri.trim() || submitting}>
          <HugeiconsIcon icon={PlusSignIcon} strokeWidth={2.2} />
          {submitting ? 'Adding' : 'Add'}
        </Button>
      </div>
      <p className="text-xs text-muted-foreground">
        Stored on this device only, in your local config folder. Never sent anywhere.
      </p>
    </form>
  )
}
