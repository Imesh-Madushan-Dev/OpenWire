/**
 * Custom window chrome, laid out the way GlassWire's is.
 *
 * The window is frameless (`decorations: false`), so this band owns dragging,
 * the main navigation, and the minimise/maximise/close controls. The app menu
 * sits top-left, the window controls top-right, the tabs in the middle, and
 * the tunnel's state bottom-right - readable from every tab, and one click
 * from the VPN page. Closing hides to the tray: the Rust side intercepts the
 * request and keeps the tunnel alive.
 */

import { useCallback } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowDown01Icon,
  Cancel01Icon,
  InformationCircleIcon,
  Mail01Icon,
  MinusSignIcon,
  SquareIcon,
} from '@hugeicons/core-free-icons'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { cn } from 'cn'

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { isBusy, type Phase } from '@/lib/types'

import { LogoMark } from './logo-mark'
import { NavTabs, type ViewId } from './nav-tabs'

const FEEDBACK_MESSAGE =
  'Found a bug or want a feature? Open an issue in the OpenWire repository - include the version from Settings.'

/**
 * The state badge, in GlassWire's upgrade-badge slot. A solid fill in the
 * state's own colour, with the page colour as ink: dark on the bright dark-mode
 * hues, white on the deep light-mode ones, 4.5:1 or better either way.
 *
 * Worded as a tunnel, which is what OpenWire is - it disguises traffic rather
 * than promising privacy - so "off" means traffic is going out directly, not
 * that the user is in danger. It still wears the warm `exposed` colour: the
 * tunnel not doing its job is worth a glance.
 */
const STATUS: Record<Phase, { label: string; fill: string }> = {
  disconnected: { label: 'Tunnel off', fill: 'bg-exposed' },
  connecting: { label: 'Connecting', fill: 'bg-primary' },
  connected: { label: 'Tunnel on', fill: 'bg-protected' },
  reconnecting: { label: 'Reconnecting', fill: 'bg-warning' },
  disconnecting: { label: 'Disconnecting', fill: 'bg-muted-foreground' },
  // Routing locally to count traffic, with nothing going through a server.
  monitoring: { label: 'Tunnel off', fill: 'bg-exposed' },
}

interface TitleBarProps {
  view: ViewId
  onNavigate: (view: ViewId) => void
  /** Opens Settings, optionally on one section. */
  onOpenSettings: (section?: 'about') => void
  badges?: Partial<Record<ViewId, number>>
  phase: Phase
  onFeedback: (message: string) => void
}

export function TitleBar({ view, onNavigate, onOpenSettings, badges, phase, onFeedback }: TitleBarProps) {
  const minimize = useCallback(() => void getCurrentWindow().minimize(), [])
  const toggleMaximize = useCallback(() => void getCurrentWindow().toggleMaximize(), [])
  // Intercepted in Rust: hides the window, leaves the tunnel running.
  const hide = useCallback(() => void getCurrentWindow().close(), [])

  const status = STATUS[phase]

  return (
    <header
      data-tauri-drag-region
      className="relative z-20 h-20 shrink-0 border-b border-hairline bg-rail"
    >
      <AppMenu onOpenSettings={onOpenSettings} onFeedback={() => onFeedback(FEEDBACK_MESSAGE)} />

      {/* Centred on the window, not on the space between the side clusters. */}
      <NavTabs
        active={view}
        onChange={onNavigate}
        badges={badges}
        className="absolute inset-y-0 left-1/2 -translate-x-1/2"
      />

      <div className="absolute top-0 right-0 flex">
        <WindowButton label="Minimize" onClick={minimize}>
          <HugeiconsIcon icon={MinusSignIcon} size={14} strokeWidth={2} />
        </WindowButton>
        <WindowButton label="Maximize" onClick={toggleMaximize}>
          <HugeiconsIcon icon={SquareIcon} size={11} strokeWidth={2} />
        </WindowButton>
        <WindowButton label="Close to tray" onClick={hide} danger>
          <HugeiconsIcon icon={Cancel01Icon} size={14} strokeWidth={2} />
        </WindowButton>
      </div>

      <button
        type="button"
        onClick={() => onNavigate('vpn')}
        aria-label={`VPN: ${status.label}. Open the VPN page`}
        title={view === 'vpn' ? undefined : 'Open the VPN page'}
        className={cn(
          'absolute right-3 bottom-3 flex h-5 items-center gap-1 rounded-[3px] px-1.5 text-[10px] font-bold tracking-wider text-background uppercase',
          'transition-[background-color,filter] duration-300 outline-none hover:brightness-110',
          'focus-visible:ring-2 focus-visible:ring-primary/60 focus-visible:ring-offset-2 focus-visible:ring-offset-rail',
          status.fill,
        )}
      >
        {/* Shape as well as colour: a dot only while the tunnel is moving. */}
        {isBusy(phase) || phase === 'reconnecting' ? (
          <span aria-hidden className="size-1.5 animate-pulse rounded-full bg-background" />
        ) : null}
        <span role="status">{status.label}</span>
      </button>
    </header>
  )
}

/**
 * GlassWire's top-left product menu: the app's mark, and the two things about
 * the app itself. Connecting, Settings and hiding all have their own controls.
 */
function AppMenu({
  onOpenSettings,
  onFeedback,
}: {
  onOpenSettings: (section?: 'about') => void
  onFeedback: () => void
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <button
            type="button"
            className="absolute top-2 left-2.5 flex h-6 items-center gap-1 rounded-[3px] bg-tint/8 pr-1 pl-0.5 text-xs font-semibold tracking-wide text-foreground/90 transition-colors outline-none hover:bg-tint/12 focus-visible:ring-2 focus-visible:ring-primary/50 data-popup-open:bg-tint/14"
          />
        }
      >
        <LogoMark size={20} />
        <span className="ml-0.5">OpenWire</span>
        <HugeiconsIcon icon={ArrowDown01Icon} size={13} strokeWidth={2} className="text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-52">
        <DropdownMenuItem onClick={onFeedback}>
          <HugeiconsIcon icon={Mail01Icon} strokeWidth={2} />
          Send feedback
        </DropdownMenuItem>
        <DropdownMenuItem onClick={() => onOpenSettings('about')}>
          <HugeiconsIcon icon={InformationCircleIcon} strokeWidth={2} />
          About OpenWire
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
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
        'grid h-7 w-10 place-items-center text-muted-foreground transition-colors',
        'focus-visible:bg-tint/10 focus-visible:outline-none',
        danger ? 'hover:bg-destructive/80 hover:text-white' : 'hover:bg-tint/10 hover:text-foreground',
      )}
    >
      {children}
    </button>
  )
}
