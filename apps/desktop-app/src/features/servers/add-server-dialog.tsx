/**
 * Add servers: paste any number of links (or a subscription body), or type
 * one server's details in. Both paths end at the same backend importer - the
 * form only assembles the link a server like that would have shared.
 */

import { useState, type FormEvent } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { ClipboardIcon } from '@hugeicons/core-free-icons'

import { Segmented } from '@/components/segmented'
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
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select'
import { Switch } from '@/components/ui/switch'
import { Textarea } from '@/components/ui/textarea'
import { useVpn } from '@/features/vpn/vpn-context'

import { FormField, ToggleField } from './form-field'
import {
  buildLink,
  PROTOCOLS,
  TRANSPORTS,
  type LinkProtocol,
  type ManualServer,
  type ManualTransport,
} from './links'

type Tab = 'paste' | 'manual'

const TABS = [
  { value: 'paste' as const, label: 'Paste links' },
  { value: 'manual' as const, label: 'Enter details' },
]

interface AddServerDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function AddServerDialog({ open, onOpenChange }: AddServerDialogProps) {
  const [tab, setTab] = useState<Tab>('paste')

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* Never taller than the window: the header, tabs and buttons stay put
          and the fields scroll between them. */}
      <DialogContent className="max-h-[calc(100dvh-2rem)] grid-cols-[minmax(0,1fr)] grid-rows-[auto_auto_minmax(0,1fr)] gap-4 sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Add a server</DialogTitle>
          <DialogDescription>
            VLESS or Trojan, over TCP, WebSocket, gRPC, XHTTP, HTTPUpgrade or HTTP/2. Links from
            your provider work as they are.
          </DialogDescription>
        </DialogHeader>

        {/* Wrapped so the grid does not stretch the control to full width. */}
        <div>
          <Segmented
            options={TABS}
            value={tab}
            onChange={setTab}
            layoutId="add-server-tab"
            aria-label="How to add"
          />
        </div>

        {tab === 'paste' ? (
          <PasteForm onDone={() => onOpenChange(false)} />
        ) : (
          <ManualForm onDone={() => onOpenChange(false)} />
        )}
      </DialogContent>
    </Dialog>
  )
}

function PasteForm({ onDone }: { onDone: () => void }) {
  const { importLinks } = useVpn()
  const [text, setText] = useState('')
  const [busy, setBusy] = useState(false)

  const run = async (source?: string) => {
    setBusy(true)
    const report = await importLinks(source)
    setBusy(false)
    if (report && report.imported.length > 0) {
      setText('')
      onDone()
    }
  }

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault()
        void run(text)
      }}
      className="flex min-h-0 flex-col gap-4"
    >
      <Textarea
        value={text}
        onChange={(event) => setText(event.target.value)}
        placeholder={'vless://…\ntrojan://…\n\nOne link per line, or a whole subscription.'}
        aria-label="Server links"
        spellCheck={false}
        autoComplete="off"
        rows={6}
        className="h-36 min-h-24 resize-none font-mono text-xs"
      />

      <DialogFooter className="sm:justify-between">
        <Button type="button" variant="ghost" disabled={busy} onClick={() => void run()}>
          <HugeiconsIcon icon={ClipboardIcon} strokeWidth={2} />
          Paste from clipboard
        </Button>
        <Button type="submit" disabled={busy || !text.trim()}>
          {busy ? 'Adding…' : 'Add'}
        </Button>
      </DialogFooter>
    </form>
  )
}

const EMPTY: ManualServer = {
  protocol: 'vless',
  name: '',
  host: '',
  port: '443',
  secret: '',
  transport: 'tcp',
  path: '',
  hostHeader: '',
  tls: true,
  sni: '',
  allowInsecure: false,
}

function ManualForm({ onDone }: { onDone: () => void }) {
  const { importServer } = useVpn()
  const [form, setForm] = useState<ManualServer>(EMPTY)
  const [busy, setBusy] = useState(false)

  const set = <K extends keyof ManualServer>(key: K, value: ManualServer[K]) =>
    setForm((current) => ({ ...current, [key]: value }))

  const protocol = PROTOCOLS.find((option) => option.value === form.protocol) ?? PROTOCOLS[0]
  const portValid =
    /^\d+$/.test(form.port.trim()) && Number(form.port) >= 1 && Number(form.port) <= 65535
  const ready = form.host.trim() !== '' && form.secret.trim() !== '' && portValid

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (!ready) return
    setBusy(true)
    const added = await importServer(buildLink(form))
    setBusy(false)
    if (added) {
      setForm(EMPTY)
      onDone()
    }
  }

  return (
    <form onSubmit={(event) => void submit(event)} className="flex min-h-0 flex-col gap-4">
      {/* The scroll area: padded by a few pixels so focus rings aren't clipped. */}
      <div className="-m-1 flex min-h-0 flex-1 flex-col gap-4 overflow-x-hidden overflow-y-auto p-1">
        <div className="flex flex-col gap-1.5">
          <div className="flex items-center justify-between gap-3">
            <Segmented
              options={PROTOCOLS}
              value={form.protocol}
              onChange={(value: LinkProtocol) => set('protocol', value)}
              layoutId="add-server-protocol"
              size="sm"
              aria-label="Protocol"
            />
            <label className="flex items-center gap-2 text-xs font-medium text-foreground/85">
              TLS
              <Switch
                aria-label="TLS"
                checked={form.tls}
                onCheckedChange={(value) => set('tls', value)}
              />
            </label>
          </div>
          {!form.tls && (
            <p className="text-[11px] leading-snug text-warning">
              No TLS: the tunnel itself isn&apos;t encrypted. Only for servers set up without it,
              usually on port 80.
            </p>
          )}
        </div>

        <div className="grid grid-cols-[1fr_96px] gap-3">
          <FormField label="Address">
            <Input
              value={form.host}
              placeholder="vpn.example.com"
              spellCheck={false}
              className="font-mono text-xs"
              onChange={(e) => set('host', e.target.value)}
            />
          </FormField>
          <FormField label="Port">
            <Input
              value={form.port}
              inputMode="numeric"
              aria-invalid={!portValid}
              className="numeric text-center"
              onChange={(e) => set('port', e.target.value)}
            />
          </FormField>
        </div>

        <FormField label={protocol.secret}>
          {/* Masked like a password: it is the credential, and screens get shared. */}
          <Input
            type="password"
            value={form.secret}
            autoComplete="off"
            spellCheck={false}
            className="font-mono text-xs"
            onChange={(e) => set('secret', e.target.value)}
          />
        </FormField>

        <div className="grid grid-cols-2 gap-3">
          <FormField label="Transport">
            <NativeSelect
              value={form.transport}
              className="w-full"
              onChange={(e) => set('transport', e.target.value as ManualTransport)}
            >
              {TRANSPORTS.map((option) => (
                <NativeSelectOption key={option.value} value={option.value}>
                  {option.label}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </FormField>
          <FormField label="Name">
            <Input
              value={form.name}
              placeholder="Optional"
              maxLength={64}
              onChange={(e) => set('name', e.target.value)}
            />
          </FormField>
        </div>

        {/* Raw TCP has no HTTP around it, so nothing to route by. */}
        {form.transport !== 'tcp' && (
          <div className="grid grid-cols-2 gap-3">
            <FormField label={form.transport === 'grpc' ? 'Service name' : 'Path'}>
              <Input
                value={form.path}
                placeholder={form.transport === 'grpc' ? 'grpc' : '/ws'}
                spellCheck={false}
                className="font-mono text-xs"
                onChange={(e) => set('path', e.target.value)}
              />
            </FormField>
            <FormField label="Host" hint="Optional. For a server behind a CDN.">
              <Input
                value={form.hostHeader}
                placeholder="cdn.example.com"
                spellCheck={false}
                className="font-mono text-xs"
                onChange={(e) => set('hostHeader', e.target.value)}
              />
            </FormField>
          </div>
        )}

        {form.tls && (
          <>
            <FormField label="Masking domain (SNI)" hint="Leave empty to use the address.">
              <Input
                value={form.sni}
                placeholder="www.microsoft.com"
                spellCheck={false}
                className="font-mono text-xs"
                onChange={(e) => set('sni', e.target.value)}
              />
            </FormField>

            <ToggleField
              label="Skip certificate check"
              description={
                form.allowInsecure
                  ? "On: the server isn't verified, so someone on your network could impersonate it."
                  : 'Only for servers you control that use a self-signed certificate.'
              }
              tone={form.allowInsecure ? 'warning' : 'default'}
              control={
                <Switch
                  aria-label="Skip certificate check"
                  checked={form.allowInsecure}
                  onCheckedChange={(value) => set('allowInsecure', value)}
                />
              }
            />
          </>
        )}
      </div>

      <DialogFooter>
        <Button type="button" variant="outline" onClick={onDone}>
          Cancel
        </Button>
        <Button type="submit" disabled={busy || !ready}>
          {busy ? 'Adding…' : 'Add server'}
        </Button>
      </DialogFooter>
    </form>
  )
}
