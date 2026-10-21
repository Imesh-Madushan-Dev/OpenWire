/**
 * Edit a server's name and how it is reached. The credential is not editable:
 * it never reaches the UI, so a new key means importing the server's new link.
 */

import { useState, type FormEvent } from 'react'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Switch } from '@/components/ui/switch'
import { isActive, type ServerEdit, type ServerSummary } from '@/lib/types'
import { useVpn } from '@/features/vpn/vpn-context'

import { FormField, ToggleField } from './form-field'

interface EditServerDialogProps {
  server: ServerSummary
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function EditServerDialog({ server, open, onOpenChange }: EditServerDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="grid-cols-[minmax(0,1fr)] gap-5">
        {/* Keyed so reopening for another server starts from its values. */}
        <EditForm key={server.id} server={server} onDone={() => onOpenChange(false)} />
      </DialogContent>
    </Dialog>
  )
}

function EditForm({ server, onDone }: { server: ServerSummary; onDone: () => void }) {
  const { updateServer, connection } = useVpn()
  const [name, setName] = useState(server.name)
  const [host, setHost] = useState(server.host)
  const [port, setPort] = useState(String(server.port))
  const [sni, setSni] = useState(server.sni)
  const [allowInsecure, setAllowInsecure] = useState(server.allowInsecure)
  const [saving, setSaving] = useState(false)

  // A live session keeps the settings it started with; the backend refuses
  // connection changes until it ends, so say so up front.
  const locked = isActive(connection.phase) && connection.serverId === server.id

  const submit = async (event: FormEvent) => {
    event.preventDefault()

    // Only what changed, so a rename never trips the in-use check.
    const edit: ServerEdit = {}
    if (name.trim() !== server.name) edit.name = name
    if (host.trim() !== server.host) edit.host = host
    if (Number(port) !== server.port) edit.port = Number(port)
    if (sni.trim() !== server.sni) edit.sni = sni
    if (allowInsecure !== server.allowInsecure) edit.allowInsecure = allowInsecure
    if (Object.keys(edit).length === 0) return onDone()

    setSaving(true)
    const saved = await updateServer(server.id, edit)
    setSaving(false)
    if (saved) onDone()
  }

  const portValid = /^\d+$/.test(port.trim()) && Number(port) >= 1 && Number(port) <= 65535

  return (
    <form onSubmit={(event) => void submit(event)} className="flex flex-col gap-5">
      <DialogHeader>
        <DialogTitle>Edit server</DialogTitle>
        <DialogDescription>
          To change its {server.protocol === 'trojan-tls' ? 'password' : 'access key'}, add the
          server&apos;s new link instead.
        </DialogDescription>
      </DialogHeader>

      <div className="flex flex-col gap-4">
        <FormField label="Name">
          <Input value={name} maxLength={64} onChange={(e) => setName(e.target.value)} />
        </FormField>

        <div className="grid grid-cols-[1fr_96px] gap-3">
          <FormField label="Address">
            <Input
              value={host}
              disabled={locked}
              spellCheck={false}
              className="font-mono text-xs"
              onChange={(e) => setHost(e.target.value)}
            />
          </FormField>
          <FormField label="Port">
            <Input
              value={port}
              disabled={locked}
              inputMode="numeric"
              aria-invalid={!portValid}
              className="numeric text-center"
              onChange={(e) => setPort(e.target.value)}
            />
          </FormField>
        </div>

        <FormField
          label="Masking domain (SNI)"
          hint="The name your connection appears to be for. It should be a site your network allows."
        >
          <Input
            value={sni}
            disabled={locked}
            spellCheck={false}
            className="font-mono text-xs"
            onChange={(e) => setSni(e.target.value)}
          />
        </FormField>

        <ToggleField
          label="Skip certificate check"
          description={
            allowInsecure
              ? "On: the server isn't verified, so someone on your network could impersonate it."
              : 'Only for servers you control that use a self-signed certificate.'
          }
          tone={allowInsecure ? 'warning' : 'default'}
          control={
            <Switch
              aria-label="Skip certificate check"
              checked={allowInsecure}
              disabled={locked}
              onCheckedChange={setAllowInsecure}
            />
          }
        />

        {locked && (
          <p className="text-xs text-warning">
            Disconnect to change the address, port or security of the server you&apos;re using.
          </p>
        )}
      </div>

      <DialogFooter>
        <Button type="button" variant="outline" onClick={onDone}>
          Cancel
        </Button>
        <Button type="submit" disabled={saving || !portValid || !name.trim() || !host.trim()}>
          {saving ? 'Saving…' : 'Save'}
        </Button>
      </DialogFooter>
    </form>
  )
}
