/**
 * The two protections worth reaching without a trip to Settings. Changing one
 * mid-session takes effect on the next connect; the backend says so.
 */

import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/react'
import { Settings01Icon, Shield01Icon, Wifi01Icon } from '@hugeicons/core-free-icons'
import { cn } from 'cn'

import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { toneStyle, type Tone } from '@/lib/tone'
import { useVpn } from '@/features/vpn/vpn-context'

export function QuickToggles({ onOpenSettings }: { onOpenSettings: () => void }) {
  const { settings, platform, updateSettings } = useVpn()
  if (!settings) return null

  const proxyUnsupported = platform ? !platform.systemProxySupported : false

  return (
    <div className="flex items-center justify-center gap-2">
      <Chip
        icon={Shield01Icon}
        tone="rose"
        label="Kill switch"
        tip="Block all traffic if the tunnel drops"
        on={settings.killSwitch}
        onToggle={() => void updateSettings({ killSwitch: !settings.killSwitch })}
      />
      <Chip
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

      <Tooltip>
        <TooltipTrigger
          render={
            <button
              type="button"
              aria-label="All settings"
              onClick={onOpenSettings}
              className="grid size-9 place-items-center rounded-full border border-tint/8 text-muted-foreground transition-colors outline-none hover:bg-tint/6 hover:text-foreground focus-visible:ring-2 focus-visible:ring-primary/50"
            />
          }
        >
          <HugeiconsIcon icon={Settings01Icon} size={14} strokeWidth={2} />
        </TooltipTrigger>
        <TooltipContent>All settings</TooltipContent>
      </Tooltip>
    </div>
  )
}

function Chip({
  icon,
  tone,
  label,
  tip,
  on,
  disabled = false,
  onToggle,
}: {
  icon: IconSvgElement
  /** The feature's identity colour, on its icon tile. */
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
            // On reads as lit, off as dim, and the dot says which; the tile's
            // colour names the feature and stays put either way.
            className={cn(
              'flex h-9 items-center gap-2 rounded-full border py-1 pr-3 pl-1 text-xs font-medium transition-colors duration-200 outline-none',
              'focus-visible:ring-2 focus-visible:ring-primary/50 disabled:opacity-50',
              on
                ? 'border-tint/14 bg-tint/8 text-foreground'
                : 'border-tint/8 bg-transparent text-muted-foreground hover:text-foreground',
            )}
          />
        }
      >
        <span
          className={cn(
            'tone-tile grid size-7 place-items-center rounded-full transition-opacity duration-200',
            !on && 'opacity-60',
          )}
          style={toneStyle(tone)}
        >
          <HugeiconsIcon icon={icon} size={14} strokeWidth={2} />
        </span>
        {label}
        <span
          aria-hidden
          className={cn(
            'size-1.5 rounded-full transition-colors',
            on ? 'bg-success' : 'bg-tint/25',
          )}
        />
      </TooltipTrigger>
      <TooltipContent>{tip}</TooltipContent>
    </Tooltip>
  )
}
