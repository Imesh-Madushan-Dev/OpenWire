/**
 * Custom window chrome.
 *
 * The window is frameless (`decorations: false`), so this bar owns dragging,
 * the main navigation, and the minimise/maximise/close controls. Closing hides
 * to the tray - the Rust side intercepts the request and keeps the tunnel alive.
 */

import { useCallback } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Cancel01Icon,
  Mail01Icon,
  MinusSignIcon,
  SquareIcon,
} from '@hugeicons/core-free-icons'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { cn } from 'cn'

import type { Phase } from '@/lib/types'

import { NavTabs, type ViewId } from './nav-tabs'

const FEEDBACK_MESSAGE =
  'Found a bug or want a feature? Open an issue in the Maizy repository - include the version from Settings.'

/** The tunnel's state, readable from every tab - not just the VPN one. */
const STATUS: Record<Phase, { label: string; dot: string }> = {
  disconnected: { label: 'VPN off', dot: 'bg-tint/30' },
  connecting: { label: 'Connecting', dot: 'bg-warning' },
  connected: { label: 'Protected', dot: 'bg-success' },
  reconnecting: { label: 'Reconnecting', dot: 'bg-warning' },
  disconnecting: { label: 'Disconnecting', dot: 'bg-tint/30' },
  monitoring: { label: 'VPN off', dot: 'bg-tint/30' },
}

interface TitleBarProps {
  view: ViewId
  onNavigate: (view: ViewId) => void
  badges?: Partial<Record<ViewId, number>>
  phase: Phase
  onFeedback: (message: string) => void
}

export function TitleBar({ view, onNavigate, badges, phase, onFeedback }: TitleBarProps) {
  const minimize = useCallback(() => void getCurrentWindow().minimize(), [])
  const toggleMaximize = useCallback(() => void getCurrentWindow().toggleMaximize(), [])
  // Intercepted in Rust: hides the window, leaves the tunnel running.
  const hide = useCallback(() => void getCurrentWindow().close(), [])

  const status = STATUS[phase]

  return (
    <header
      data-tauri-drag-region
      className="relative z-20 flex h-14 shrink-0 items-center border-b border-hairline bg-rail pr-1.5 pl-4"
    >
      {/* Wordmark and status double as a drag handle. */}
      <div data-tauri-drag-region className="pointer-events-none flex items-center gap-3">
        <span className="text-[13px] font-semibold tracking-wide text-foreground/85">maizy</span>
        <span
          role="status"
          className="flex items-center gap-1.5 rounded-full bg-tint/6 px-2 py-0.5 text-[11px] font-medium text-muted-foreground"
        >
          <span aria-hidden className={cn('size-1.5 rounded-full transition-colors', status.dot)} />
          {status.label}
        </span>
      </div>

      {/* Centred on the window, not on the space between the side clusters. */}
      <NavTabs
        active={view}
        onChange={onNavigate}
        badges={badges}
        className="absolute inset-y-0 left-1/2 -translate-x-1/2"
      />

      <div className="ml-auto flex items-center gap-1">
        <button
          type="button"
          onClick={() => onFeedback(FEEDBACK_MESSAGE)}
          className="mr-1 flex h-7 items-center gap-1.5 rounded-full px-3 text-xs font-medium text-muted-foreground transition-colors hover:bg-tint/6 hover:text-foreground focus-visible:ring-2 focus-visible:ring-primary/50 focus-visible:outline-none"
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
      </div>
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
        danger ? 'hover:bg-destructive/80 hover:text-white' : 'hover:bg-tint/8 hover:text-foreground',
      )}
    >
      {children}
    </button>
  )
}
