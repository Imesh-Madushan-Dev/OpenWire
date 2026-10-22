/**
 * The connection log: every step of connecting, staying connected and
 * disconnecting, as the backend reports it (`core/connlog.rs`).
 *
 * A footer under the server list shows the latest line and opens the full
 * log, the way NetMod and v2rayN users expect to check why a server will not
 * connect. The log lives in memory on the Rust side, so it is all still here
 * when the window is reopened from the tray.
 */

import { useEffect, useRef, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Alert02Icon,
  CancelCircleIcon,
  CheckmarkCircle02Icon,
  CommandLineIcon,
  Copy01Icon,
  Delete02Icon,
  Tick02Icon,
} from '@hugeicons/core-free-icons'
import { cn } from 'cn'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import type { LogEntry } from '@/lib/types'
import { useVpn } from '@/features/vpn/vpn-context'

const LEVEL_TEXT: Record<LogEntry['level'], string> = {
  info: 'text-foreground/85',
  success: 'text-success',
  warning: 'text-warning',
  error: 'text-destructive',
}

const LEVEL_DOT: Record<LogEntry['level'], string> = {
  info: 'bg-muted-foreground/60',
  success: 'bg-success',
  warning: 'bg-warning',
  error: 'bg-destructive',
}

const LEVEL_ICON = {
  success: CheckmarkCircle02Icon,
  warning: Alert02Icon,
  error: CancelCircleIcon,
} as const

/** The bar under the server list: the latest line, and the way into the rest. */
export function LogFooter({ latest, onOpen }: { latest: LogEntry | undefined; onOpen: () => void }) {
  return (
    <button
      type="button"
      onClick={onOpen}
      className="flex h-9 shrink-0 items-center gap-2 border-t border-hairline px-3 text-left text-xs transition-colors outline-none hover:bg-tint/6 focus-visible:bg-tint/8"
    >
      <HugeiconsIcon icon={CommandLineIcon} size={14} strokeWidth={2} className="shrink-0 text-muted-foreground" />
      <span className="shrink-0 font-medium">Logs</span>
      {latest ? (
        <>
          <span aria-hidden className={cn('size-1.5 shrink-0 rounded-full', LEVEL_DOT[latest.level])} />
          <span className="min-w-0 flex-1 truncate text-muted-foreground">{latest.message}</span>
        </>
      ) : (
        <span className="min-w-0 flex-1 truncate text-muted-foreground">Nothing yet</span>
      )}
      <span className="shrink-0 text-[11px] text-muted-foreground/80">View</span>
    </button>
  )
}

export function LogDialog({
  entries,
  open,
  onOpenChange,
  onClear,
}: {
  entries: LogEntry[]
  open: boolean
  onOpenChange: (open: boolean) => void
  onClear: () => Promise<void>
}) {
  const { notify } = useVpn()
  const [copied, setCopied] = useState(false)
  const scroller = useRef<HTMLOListElement>(null)
  // Follows new lines only while the reader is at the bottom; scrolling up to
  // read an earlier line stops the jump.
  const following = useRef(true)

  useEffect(() => {
    const list = scroller.current
    if (open && list && following.current) list.scrollTop = list.scrollHeight
  }, [entries, open])

  const copy = async () => {
    const text = entries
      .map((entry) => `[${clock(entry.ts)}] ${entry.level.toUpperCase().padEnd(7)} ${entry.message}`)
      .join('\n')
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1800)
    } catch {
      notify({ level: 'error', message: 'Could not copy to the clipboard.' })
    }
  }

  const clear = () =>
    void onClear().catch(() => notify({ level: 'error', message: 'Could not clear the log.' }))

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (next) following.current = true
        onOpenChange(next)
      }}
    >
      <DialogContent className="flex h-[min(26rem,calc(100vh-3rem))] flex-col gap-3 sm:max-w-2xl">
        <DialogHeader className="flex-row items-start gap-3 pr-8">
          <div className="min-w-0 flex-1">
            <DialogTitle>Connection log</DialogTitle>
            <DialogDescription>Every step of connecting, in order. Cleared when OpenWire quits.</DialogDescription>
          </div>
          <div className="flex shrink-0 gap-1">
            <Button variant="outline" size="xs" disabled={entries.length === 0} onClick={() => void copy()}>
              <HugeiconsIcon icon={copied ? Tick02Icon : Copy01Icon} strokeWidth={2} />
              {copied ? 'Copied' : 'Copy'}
            </Button>
            <Button variant="outline" size="xs" disabled={entries.length === 0} onClick={clear}>
              <HugeiconsIcon icon={Delete02Icon} strokeWidth={2} />
              Clear
            </Button>
          </div>
        </DialogHeader>

        {entries.length === 0 ? (
          <div className="grid min-h-0 flex-1 place-items-center rounded-lg bg-tint/5 px-6 text-center text-xs text-muted-foreground">
            Nothing logged yet. Connect to a server and each step will show here.
          </div>
        ) : (
          <ol
            // Mounts after the dialog opens, so it scrolls itself to the
            // newest line rather than waiting for the effect.
            ref={(node) => {
              scroller.current = node
              if (node && following.current) node.scrollTop = node.scrollHeight
            }}
            aria-label="Connection log"
            onScroll={(event) => {
              const list = event.currentTarget
              following.current = list.scrollHeight - list.scrollTop - list.clientHeight < 24
            }}
            className="min-h-0 flex-1 overflow-y-auto rounded-lg bg-tint/5 py-2 font-mono text-[11px] leading-normal"
            data-selectable
          >
            {entries.map((entry, index) => (
              <LogLine key={entry.id} entry={entry} divided={entry.sessionStart && index > 0} />
            ))}
          </ol>
        )}
      </DialogContent>
    </Dialog>
  )
}

function LogLine({ entry, divided }: { entry: LogEntry; divided: boolean }) {
  const icon = entry.level === 'info' ? null : LEVEL_ICON[entry.level]

  return (
    <li
      className={cn(
        'flex gap-2.5 px-3 py-0.5',
        divided && 'mt-2 border-t border-hairline pt-2.5',
        entry.sessionStart && 'font-semibold',
      )}
    >
      <time className="shrink-0 text-muted-foreground/80 tabular-nums" dateTime={new Date(entry.ts).toISOString()}>
        {clock(entry.ts)}
      </time>
      <span className="grid w-3.5 shrink-0 place-items-center pt-px">
        {icon && <HugeiconsIcon icon={icon} size={13} strokeWidth={2} className={LEVEL_TEXT[entry.level]} />}
      </span>
      <span className={cn('min-w-0 wrap-break-word', LEVEL_TEXT[entry.level])}>{entry.message}</span>
    </li>
  )
}

/** 24-hour time with seconds, as logs are read. */
function clock(ts: number): string {
  return new Date(ts).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  })
}
