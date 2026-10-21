/**
 * Floating navigation dock, centred over the bottom of the window.
 *
 * It overlays the views rather than taking a strip of its own, so each view
 * leaves `pb-dock` of clearance where its last content would otherwise sit
 * underneath it.
 */

import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/react'
import {
  ChartRingIcon,
  DashboardSpeed02Icon,
  Settings01Icon,
  ShieldEnergyIcon,
} from '@hugeicons/core-free-icons'
import { AnimatePresence, motion } from 'motion/react'
import { cn } from 'cn'

import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { spring, transition } from '@/lib/motion'

export type ViewId = 'vpn' | 'network' | 'usage' | 'settings'

interface NavItem {
  id: ViewId
  label: string
  icon: IconSvgElement
}

// Distinct silhouettes - shield, gauge, ring, gear - so each item reads at a
// glance without the label.
const PRIMARY: NavItem[] = [
  { id: 'vpn', label: 'VPN', icon: ShieldEnergyIcon },
  { id: 'network', label: 'Network', icon: DashboardSpeed02Icon },
  { id: 'usage', label: 'Usage', icon: ChartRingIcon },
]

const SECONDARY: NavItem[] = [{ id: 'settings', label: 'Settings', icon: Settings01Icon }]

interface DockProps {
  active: ViewId
  onChange: (view: ViewId) => void
  /** Drives the status pip on the VPN item. */
  connected: boolean
}

export function Dock({ active, onChange, connected }: DockProps) {
  const button = (item: NavItem) => (
    <DockButton
      key={item.id}
      item={item}
      active={active === item.id}
      onClick={() => onChange(item.id)}
      connected={item.id === 'vpn' && connected}
    />
  )

  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-3 z-30 flex justify-center">
      <nav
        aria-label="Main"
        className="pointer-events-auto flex items-center gap-1 rounded-full border border-tint/10 bg-card/80 p-1.5 shadow-2xl shadow-black/30 backdrop-blur-xl"
      >
        {PRIMARY.map(button)}
        <span aria-hidden className="mx-1 h-6 w-px bg-tint/10" />
        {SECONDARY.map(button)}
      </nav>
    </div>
  )
}

function DockButton({
  item,
  active,
  onClick,
  connected,
}: {
  item: NavItem
  active: boolean
  onClick: () => void
  connected: boolean
}) {
  return (
    // Only the active item spells out its name; the rest rely on the tooltip.
    <Tooltip disabled={active}>
      <TooltipTrigger
        render={
          <button
            type="button"
            onClick={onClick}
            aria-current={active ? 'page' : undefined}
            aria-label={item.label}
            className="group relative flex h-10 items-center rounded-full px-3 outline-none transition-colors duration-200 hover:bg-tint/6 focus-visible:ring-2 focus-visible:ring-primary/50"
          />
        }
      >
        {/* One shared element slides between items instead of fading per button. */}
        {active && (
          <motion.span
            layoutId="dock-active"
            aria-hidden
            className="absolute inset-0 rounded-full bg-primary/14"
            transition={spring.soft}
          />
        )}

        <span className="relative flex">
          <HugeiconsIcon
            icon={item.icon}
            size={20}
            strokeWidth={1.8}
            className={cn(
              'transition-colors duration-200',
              active ? 'text-primary' : 'text-muted-foreground group-hover:text-foreground/80',
            )}
          />
          {connected && (
            <motion.span
              aria-hidden
              className="absolute -top-0.5 -right-1 size-2.5 rounded-full border-2 border-card bg-primary"
              animate={{ scale: [1, 1.25, 1] }}
              transition={{ duration: 2.2, repeat: Infinity, ease: 'easeInOut' }}
            />
          )}
        </span>

        <AnimatePresence initial={false}>
          {active && (
            <motion.span
              initial={{ width: 0, opacity: 0 }}
              animate={{ width: 'auto', opacity: 1 }}
              exit={{ width: 0, opacity: 0 }}
              transition={transition.base}
              className="relative overflow-hidden text-xs font-semibold whitespace-nowrap text-foreground"
            >
              <span className="pl-2">{item.label}</span>
            </motion.span>
          )}
        </AnimatePresence>
      </TooltipTrigger>
      <TooltipContent sideOffset={10}>{item.label}</TooltipContent>
    </Tooltip>
  )
}
