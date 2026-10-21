/**
 * A device's details, with the two things worth changing: a name the user
 * recognises, and its type when the guess was wrong.
 */

import { useState, type FormEvent } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { cn } from 'cn'

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
import type { Device, DeviceKind, NameSource } from '@/lib/types'
import { FormField } from '@/features/servers/form-field'

import { useDevices } from './devices-context'
import { deviceTitle, KIND_ORDER, KINDS } from './device-kinds'

const SOURCE: Record<NameSource, string> = {
  mdns: 'over Bonjour',
  netbios: 'over NetBIOS',
  dns: 'to your router',
}

interface DeviceDialogProps {
  device: Device | null
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function DeviceDialog({ device, open, onOpenChange }: DeviceDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="grid-cols-[minmax(0,1fr)] gap-5">
        {/* Keyed so reopening for another device starts from its values. */}
        {device && <DeviceForm key={device.mac} device={device} onDone={() => onOpenChange(false)} />}
      </DialogContent>
    </Dialog>
  )
}

function DeviceForm({ device, onDone }: { device: Device; onDone: () => void }) {
  const { rename, setKind } = useDevices()
  const [name, setName] = useState(device.customName ?? '')
  // `null` is "Automatic": whatever the evidence suggests.
  const savedKind = device.kindIsCustom ? device.kind : null
  const [kind, setKindChoice] = useState<DeviceKind | null>(savedKind)
  const [saving, setSaving] = useState(false)

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    const nextName = name.trim() || null
    setSaving(true)
    let ok = true
    if (nextName !== device.customName) ok = await rename(device.mac, nextName)
    if (ok && kind !== savedKind) ok = await setKind(device.mac, kind)
    setSaving(false)
    if (ok) onDone()
  }

  // The automatic tile shows the current guess, when there is one to show.
  const autoIcon = KINDS[device.kindIsCustom ? 'unknown' : device.kind].icon

  return (
    <form onSubmit={(event) => void submit(event)} className="flex flex-col gap-5">
      <DialogHeader>
        <DialogTitle>{deviceTitle(device)}</DialogTitle>
        <DialogDescription className="font-mono text-xs" data-selectable>
          {device.ip} · {device.mac}
        </DialogDescription>
      </DialogHeader>

      <div className="flex flex-col gap-4">
        <FormField
          label="Name"
          hint={
            device.hostname && device.nameSource
              ? `It calls itself "${device.hostname}" ${SOURCE[device.nameSource]}. Leave blank to use that.`
              : "It didn't give a name. Pick one you'll recognise."
          }
        >
          <Input
            value={name}
            maxLength={64}
            placeholder={device.hostname ?? deviceTitle(device)}
            onChange={(event) => setName(event.target.value)}
          />
        </FormField>

        <div className="flex flex-col gap-1.5">
          <span className="text-xs font-medium text-foreground/85">Type</span>
          <div role="radiogroup" aria-label="Type" className="grid grid-cols-5 gap-1.5">
            <KindTile label="Automatic" icon={autoIcon} selected={kind === null} onSelect={() => setKindChoice(null)} />
            {KIND_ORDER.map((option) => (
              <KindTile
                key={option}
                label={KINDS[option].label}
                icon={KINDS[option].icon}
                selected={kind === option}
                onSelect={() => setKindChoice(option)}
              />
            ))}
          </div>
        </div>

        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 rounded-2xl bg-tint/4 px-3 py-2.5 text-xs">
          <dt className="text-muted-foreground">Maker</dt>
          <dd>{device.vendor ?? (device.privateAddress ? 'Hidden (private address)' : 'Unknown')}</dd>
          <dt className="text-muted-foreground">First seen</dt>
          <dd className="numeric">{new Date(device.firstSeen * 1000).toLocaleString()}</dd>
          <dt className="text-muted-foreground">Last seen</dt>
          <dd className="numeric">
            {device.online ? 'Online now' : new Date(device.lastSeen * 1000).toLocaleString()}
          </dd>
        </dl>
      </div>

      <DialogFooter>
        <Button type="button" variant="outline" onClick={onDone}>
          Cancel
        </Button>
        <Button type="submit" disabled={saving}>
          {saving ? 'Saving…' : 'Save'}
        </Button>
      </DialogFooter>
    </form>
  )
}

function KindTile({
  label,
  icon,
  selected,
  onSelect,
}: {
  label: string
  icon: (typeof KINDS)[DeviceKind]['icon']
  selected: boolean
  onSelect: () => void
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      onClick={onSelect}
      className={cn(
        'flex flex-col items-center gap-1 rounded-xl border px-1 py-2 text-[10px] leading-tight font-medium transition-colors outline-none',
        'focus-visible:ring-2 focus-visible:ring-primary/50',
        selected
          ? 'border-primary/60 bg-primary/10 text-foreground'
          : 'border-tint/8 text-muted-foreground hover:bg-tint/5 hover:text-foreground',
      )}
    >
      <HugeiconsIcon icon={icon} size={17} strokeWidth={1.7} />
      <span className="line-clamp-2 text-center">{label}</span>
    </button>
  )
}
