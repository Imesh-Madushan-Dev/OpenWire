/**
 * The server list beside the connect control, as a flat GlassWire column.
 *
 * Every way of adding a server lands in the same backend importer: the Add
 * menu (clipboard, link file, QR image, or the dialog to paste links or type
 * details in), a file or image dropped on the panel, and Ctrl+V anywhere on
 * the page.
 */

import { useEffect, useEffectEvent, useMemo, useRef, useState, type DragEvent } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ClipboardIcon,
  FileImportIcon,
  ImageUpload01Icon,
  PencilEdit02Icon,
  PlusSignIcon,
  QrCodeIcon,
  RefreshIcon,
  Search01Icon,
  ServerStack01Icon,
} from '@hugeicons/core-free-icons'
import { AnimatePresence, motion } from 'motion/react'

import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Input } from '@/components/ui/input'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { staggerParent } from '@/lib/motion'
import { isActive, type ServerSummary } from '@/lib/types'
import { useVpn } from '@/features/vpn/vpn-context'

import { AddServerDialog } from './add-server-dialog'
import { LogDialog, LogFooter } from './connection-log'
import { EditServerDialog } from './edit-server-dialog'
import { readQrImage, readTextFile } from './links'
import { AutomaticRow, ServerRow } from './server-row'
import { ShareDialog } from './share-dialog'
import { useConnectionLog } from './use-connection-log'

/** Searching only earns its space once the list outgrows a glance. */
const SEARCH_FROM = 7

/** One dialog at a time, kept mounted while it animates closed. */
type DialogState = { kind: 'share' | 'edit' | 'delete'; server: ServerSummary; open: boolean }

export function ServerPanel() {
  const {
    servers,
    settings,
    connection,
    optimal,
    chooseServer,
    connect,
    disconnect,
    probeServers,
    probing,
    removeServer,
    importLinks,
    notify,
  } = useVpn()

  const [adding, setAdding] = useState(false)
  const [dialog, setDialog] = useState<DialogState | null>(null)
  const [logOpen, setLogOpen] = useState(false)
  const log = useConnectionLog()
  const [query, setQuery] = useState('')
  const [dragging, setDragging] = useState(false)
  const fileInput = useRef<HTMLInputElement>(null)
  const imageInput = useRef<HTMLInputElement>(null)

  const list = useMemo(() => {
    const needle = query.trim().toLowerCase()
    const usable = servers.filter((server) => server.protocol !== 'direct')
    const matches = needle
      ? usable.filter((server) =>
          `${server.name} ${server.host}`.toLowerCase().includes(needle),
        )
      : usable

    // Fastest first. Unmeasured servers sort last rather than pretending to
    // be instant, then by name so the order is stable between pings.
    return [...matches].sort(
      (a, b) =>
        (a.latencyMs ?? Number.MAX_SAFE_INTEGER) - (b.latencyMs ?? Number.MAX_SAFE_INTEGER) ||
        a.name.localeCompare(b.name),
    )
  }, [servers, query])

  const total = servers.filter((server) => server.protocol !== 'direct').length
  const preferred = settings?.preferredServer ?? null

  // Choosing moves a live session over by itself; otherwise connect as well.
  // The server already in use is reconnected, and the saved choice - which
  // may be Automatic - is left as it was.
  const connectTo = async (id: string | null) => {
    if (isActive(connection.phase) && id !== null && id === connection.serverId) {
      await disconnect()
      await connect(id)
      return
    }
    await chooseServer(id)
    if (!isActive(connection.phase)) await connect(id)
  }

  const importFile = async (file: File) => {
    try {
      const text = file.type.startsWith('image/') ? await readQrImage(file) : await readTextFile(file)
      await importLinks(text)
    } catch (error) {
      notify({
        level: 'error',
        message: error instanceof Error ? error.message : 'Could not read that file.',
      })
    }
  }

  // Ctrl+V anywhere on the page imports, unless the user is typing into a
  // field. The paste event hands over the clipboard without the permission
  // prompt a programmatic read would raise - and it carries pasted
  // screenshots too, so a copied QR code works as well as a copied link.
  // An effect event, so the listener is attached once yet always sees the
  // latest importers.
  const onPaste = useEffectEvent((event: ClipboardEvent) => {
    const target = event.target as HTMLElement | null
    if (target?.closest('input, textarea, [contenteditable="true"]')) return
    // A dialog is open; its own fields own the paste.
    if (document.querySelector('[role="dialog"], [role="alertdialog"]')) return

    const image = [...(event.clipboardData?.files ?? [])].find((file) =>
      file.type.startsWith('image/'),
    )
    const text = event.clipboardData?.getData('text') ?? ''
    if (!image && !text.trim()) return

    event.preventDefault()
    void (image ? importFile(image) : importLinks(text))
  })

  useEffect(() => {
    const listener = (event: ClipboardEvent) => onPaste(event)
    window.addEventListener('paste', listener)
    return () => window.removeEventListener('paste', listener)
  }, [])

  const onDrop = (event: DragEvent) => {
    event.preventDefault()
    setDragging(false)
    for (const file of event.dataTransfer.files) void importFile(file)
  }

  const closeDialog = (open: boolean) => {
    if (!open) setDialog((current) => current && { ...current, open: false })
  }

  return (
    <aside
      aria-label="Servers"
      onDragOver={(event) => {
        if (!event.dataTransfer.types.includes('Files')) return
        event.preventDefault()
        setDragging(true)
      }}
      onDragLeave={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDragging(false)
      }}
      onDrop={onDrop}
      className="relative flex h-full w-72 shrink-0 flex-col border-l border-hairline"
    >
      <header className="flex h-10 shrink-0 items-center gap-1 pr-2 pl-3">
        <div className="min-w-0 flex-1">
          <h2 className="text-xs font-medium text-muted-foreground">
            Servers{total > 0 && <span className="numeric ml-1.5 text-muted-foreground/70">{total}</span>}
          </h2>
          <p className="truncate text-[10px] text-muted-foreground/70">Ctrl+V adds a copied link</p>
        </div>

        <Tooltip>
          <TooltipTrigger
            render={
              <Button
                variant="ghost"
                size="icon-xs"
                aria-label="Ping all servers"
                disabled={probing || total === 0}
                onClick={() => void probeServers()}
              />
            }
          >
            <HugeiconsIcon
              icon={RefreshIcon}
              strokeWidth={2}
              className={probing ? 'animate-spin' : undefined}
            />
          </TooltipTrigger>
          <TooltipContent>{probing ? 'Pinging…' : 'Ping all servers'}</TooltipContent>
        </Tooltip>

        <DropdownMenu>
          <DropdownMenuTrigger render={<Button size="xs" className="ml-0.5" />}>
            <HugeiconsIcon icon={PlusSignIcon} strokeWidth={2.2} />
            Add
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-56">
            <DropdownMenuItem onClick={() => void importLinks()}>
              <HugeiconsIcon icon={ClipboardIcon} strokeWidth={2} />
              From clipboard
              <DropdownMenuShortcut>Ctrl V</DropdownMenuShortcut>
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => fileInput.current?.click()}>
              <HugeiconsIcon icon={FileImportIcon} strokeWidth={2} />
              From file…
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => imageInput.current?.click()}>
              <HugeiconsIcon icon={QrCodeIcon} strokeWidth={2} />
              From QR code image…
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={() => setAdding(true)}>
              <HugeiconsIcon icon={PencilEdit02Icon} strokeWidth={2} />
              Paste or enter details…
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </header>

      {total >= SEARCH_FROM && (
        <div className="px-3 pb-2">
          <div className="relative">
            <HugeiconsIcon
              icon={Search01Icon}
              size={14}
              strokeWidth={2}
              className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-muted-foreground"
            />
            <Input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search servers"
              aria-label="Search servers"
              className="h-7 pl-8 text-xs"
            />
          </div>
        </div>
      )}

      {total === 0 ? (
        <EmptyState onPaste={() => void importLinks()} onAdd={() => setAdding(true)} />
      ) : (
        <motion.ol
          role="radiogroup"
          aria-label="Server to use"
          variants={staggerParent}
          initial="initial"
          animate="animate"
          className="flex min-h-0 flex-1 flex-col overflow-y-auto border-t border-hairline pb-2"
        >
          {!query && (
            <AutomaticRow
              selected={preferred === null}
              fastest={optimal}
              onSelect={() => void chooseServer(null)}
              onConnect={() => void connectTo(null)}
            />
          )}
          <AnimatePresence initial={false}>
            {list.map((server) => (
              <ServerRow
                key={server.id}
                server={server}
                selected={server.id === preferred}
                inUse={server.id === connection.serverId}
                onSelect={() => void chooseServer(server.id)}
                onConnect={() => void connectTo(server.id)}
                onShare={() => setDialog({ kind: 'share', server, open: true })}
                onEdit={() => setDialog({ kind: 'edit', server, open: true })}
                onRemove={() => setDialog({ kind: 'delete', server, open: true })}
              />
            ))}
          </AnimatePresence>
          {list.length === 0 && (
            <li className="px-3 py-8 text-center text-xs text-muted-foreground">
              No servers match &ldquo;{query}&rdquo;.
            </li>
          )}
        </motion.ol>
      )}

      <LogFooter latest={log.entries.at(-1)} onOpen={() => setLogOpen(true)} />

      {/* Shown while a file is dragged over, so dropping is discoverable. */}
      <AnimatePresence>
        {dragging && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            className="pointer-events-none absolute inset-2 grid place-items-center rounded-lg border-2 border-dashed border-primary/50 bg-background/80 backdrop-blur-sm"
          >
            <span className="flex flex-col items-center gap-2 text-center text-sm font-medium">
              <HugeiconsIcon icon={ImageUpload01Icon} size={22} strokeWidth={1.8} className="text-primary" />
              Drop a link file or QR image
            </span>
          </motion.div>
        )}
      </AnimatePresence>

      <input
        ref={fileInput}
        type="file"
        accept=".txt,.conf,.list,text/plain"
        hidden
        onChange={(event) => {
          const file = event.target.files?.[0]
          event.target.value = ''
          if (file) void importFile(file)
        }}
      />
      <input
        ref={imageInput}
        type="file"
        accept="image/*"
        hidden
        onChange={(event) => {
          const file = event.target.files?.[0]
          event.target.value = ''
          if (file) void importFile(file)
        }}
      />

      <AddServerDialog open={adding} onOpenChange={setAdding} />
      <LogDialog entries={log.entries} open={logOpen} onOpenChange={setLogOpen} onClear={log.clear} />

      {dialog?.kind === 'share' && (
        <ShareDialog server={dialog.server} open={dialog.open} onOpenChange={closeDialog} />
      )}
      {dialog?.kind === 'edit' && (
        <EditServerDialog server={dialog.server} open={dialog.open} onOpenChange={closeDialog} />
      )}
      {dialog?.kind === 'delete' && (
        <DeleteDialog
          server={dialog.server}
          open={dialog.open}
          inUse={dialog.server.id === connection.serverId}
          onOpenChange={closeDialog}
          onConfirm={() => {
            void removeServer(dialog.server.id)
            closeDialog(false)
          }}
        />
      )}
    </aside>
  )
}

function EmptyState({ onPaste, onAdd }: { onPaste: () => void; onAdd: () => void }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 border-t border-hairline px-6 pb-8 text-center">
      <span className="grid size-11 place-items-center rounded-lg bg-tint/7 text-muted-foreground">
        <HugeiconsIcon icon={ServerStack01Icon} size={22} strokeWidth={1.8} />
      </span>
      <div className="flex flex-col gap-1">
        <p className="text-sm font-semibold">Add your first server</p>
        <p className="text-xs leading-relaxed text-muted-foreground">
          Paste a vless:// or trojan:// link, drop a link file or QR image here, or enter the
          details yourself.
        </p>
      </div>
      <div className="flex gap-2">
        <Button size="sm" onClick={onPaste}>
          <HugeiconsIcon icon={ClipboardIcon} strokeWidth={2} />
          Paste link
        </Button>
        <Button size="sm" variant="outline" onClick={onAdd}>
          Enter details
        </Button>
      </div>
    </div>
  )
}

function DeleteDialog({
  server,
  open,
  inUse,
  onOpenChange,
  onConfirm,
}: {
  server: ServerSummary
  open: boolean
  inUse: boolean
  onOpenChange: (open: boolean) => void
  onConfirm: () => void
}) {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent size="sm">
        <AlertDialogHeader>
          <AlertDialogTitle>Delete {server.name}?</AlertDialogTitle>
          <AlertDialogDescription>
            {inUse
              ? 'Disconnect first - this is the server you are using.'
              : "You'll need its link to add it again."}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <Button variant="destructive" disabled={inUse} onClick={onConfirm}>
            Delete
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
