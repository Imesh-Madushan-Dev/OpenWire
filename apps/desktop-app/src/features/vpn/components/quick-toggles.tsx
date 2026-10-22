/**
 * The two protections worth reaching without a trip to Settings, as toolbar
 * toggles. Changing one mid-session takes effect on the next connect; the
 * backend says so.
 */

import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/react'
import { Shield01Icon, Wifi01Icon } from '@hugeicons/core-free-icons'
import { cn } from 'cn'

import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { toneStyle, type Tone } from '@/lib/tone'
import { useVpn } from '@/features/vpn/vpn-context'

export function QuickToggles() {
  const { settings, platform, updateSettings } = useVpn()
  if (!settings) return null

  const proxyUnsupported = platform ? !platform.systemProxySupported : false

  return (
    <div className="flex items-center gap-0.5">
      <Toggle
        icon={Shield01Icon}
        tone="rose"
        label="Kill switch"
        tip="Block all traffic if the tunnel drops"
        on={settings.killSwitch}
        onToggle={() => void updateSettings({ killSwitch: !settings.killSwitch })}
      />
      <Toggle
        icon={Wifi01Icon}
        tone="sky"
        label="All apps"
        tip={
          proxyUnsupported
            ? 'Not available on this platform yet'
            : 'Send every app through the tunnel, not just ones you point at it'
        }
        on={settings.systemProxy && !proxyUnsupported}
        disabled={proxyUnsupported}
        onToggle={() => void updateSettings({ systemProxy: !settings.systemProxy })}
      />
    </div>
  )
}

function Toggle({
  icon,
  tone,
  label,
  tip,
  on,
  disabled = false,
  onToggle,
}: {
  icon: IconSvgElement
  /** The feature's identity colour, on its icon. */
  tone: Tone
  label: string
  tip: string
  on: boolean
  disabled?: boolean
  onToggle: () => void
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            type="button"
            aria-pressed={on}
            disabled={disabled}
            onClick={onToggle}
            // The icon's colour names the feature and stays put; on and off
            // are told apart by the lit chip, the switch and the brightness.
            className={cn(
              'flex h-6 items-center gap-1.5 rounded-[3px] pr-1.5 pl-1.5 text-xs font-medium transition-colors duration-150 outline-none',
              'focus-visible:ring-2 focus-visible:ring-primary/50 disabled:opacity-50',
              on ? 'bg-tint/12 text-foreground' : 'text-foreground/70 hover:bg-tint/6 hover:text-foreground',
            )}
          />
        }
      >
        <span className="tone-tile grid size-4.5 place-items-center rounded-[3px]" style={toneStyle(tone)}>
          <HugeiconsIcon icon={icon} size={11} strokeWidth={2.2} />
        </span>
        {label}
        {/* A miniature switch: shape, not just colour, says which state. */}
        <span
          aria-hidden
          className={cn(
            'relative ml-0.5 h-3 w-5 rounded-full transition-colors duration-200',
            on ? 'bg-success' : 'bg-tint/20',
          )}
        >
          <span
            className={cn(
              'absolute top-0.5 size-2 rounded-full bg-background transition-[left] duration-200',
              on ? 'left-2.5' : 'left-0.5',
            )}
          />
        </span>
      </TooltipTrigger>
      <TooltipContent>{tip}</TooltipContent>
    </Tooltip>
  )
}
