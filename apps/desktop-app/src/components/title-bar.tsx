/**
 * Custom window chrome.
 *
 * The window is frameless (`decorations: false`), so this bar owns dragging and
 * the minimise/maximise/close controls. Closing hides to the tray - the Rust
 * side intercepts the request and keeps the tunnel alive.
 */

import { useCallback } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Cancel01Icon, Mail01Icon, MinusSignIcon, SquareIcon } from '@hugeicons/core-free-icons'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { cn } from 'cn'

const FEEDBACK_MESSAGE =
  'Found a bug or want a feature? Open an issue in the Maizy repository - include the version from Settings.'

export function TitleBar({ onFeedback }: { onFeedback: (message: string) => void }) {
  const minimize = useCallback(() => void getCurrentWindow().minimize(), [])
  const toggleMaximize = useCallback(() => void getCurrentWindow().toggleMaximize(), [])
  // Intercepted in Rust: hides the window, leaves the tunnel running.
  const hide = useCallback(() => void getCurrentWindow().close(), [])

  return (
    <header
      data-tauri-drag-region
      className="relative z-20 flex h-11 shrink-0 items-center justify-end gap-1 pr-1 pl-4"
    >
      {/* Wordmark doubles as a drag handle. */}
      <span
        data-tauri-drag-region
        className="pointer-events-none absolute left-4 text-xs font-semibold tracking-wide text-foreground/70"
      >
        maizy
      </span>

      <button
        type="button"
        onClick={() => onFeedback(FEEDBACK_MESSAGE)}
        className="mr-1 flex h-7 items-center gap-1.5 rounded-full px-3 text-xs font-medium text-muted-foreground transition-colors hover:bg-white/6 hover:text-foreground focus-visible:ring-2 focus-visible:ring-primary/50 focus-visible:outline-none"
      >
        <HugeiconsIcon icon={Mail01Icon} size={13} strokeWidth={2} />
        Feedback
      </button>

      <WindowButton label="Minimize" onClick={minimize}>
        <HugeiconsIcon icon={MinusSignIcon} size={14} strokeWidth={2.2} />
      </WindowButton>
      <WindowButton label="Maximize" onClick={toggleMaximize}>
        <HugeiconsIcon icon={SquareIcon} size={11} strokeWidth={2.2} />
      </WindowButton>
      <WindowButton label="Close to tray" onClick={hide} danger>
        <HugeiconsIcon icon={Cancel01Icon} size={14} strokeWidth={2.2} />
      </WindowButton>
    </header>
  )
}

function WindowButton({
  label,
  onClick,
  children,
  danger = false,
}: {
  label: string
  onClick: () => void
  children: React.ReactNode
  danger?: boolean
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={cn(
        'grid size-8 place-items-center rounded-lg text-muted-foreground transition-colors',
        'focus-visible:ring-2 focus-visible:ring-primary/50 focus-visible:outline-none',
        danger ? 'hover:bg-destructive/80 hover:text-white' : 'hover:bg-white/8 hover:text-foreground',
      )}
    >
      {children}
    </button>
  )
}
