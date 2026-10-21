/**
 * Main navigation: icon-over-label tabs centred in the title bar.
 *
 * The active tab is told apart by brightness first - a lit icon and label
 * against muted ones - and by the underline second, so it never rests on
 * colour alone.
 */

import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/react'
import {
  ChartRingIcon,
  DashboardSpeed02Icon,
  Settings01Icon,
  ShieldEnergyIcon,
} from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'
import { cn } from 'cn'

import { spring } from '@/lib/motion'

export type ViewId = 'vpn' | 'network' | 'usage' | 'settings'

interface NavItem {
  id: ViewId
  label: string
  icon: IconSvgElement
}

// Distinct silhouettes - shield, gauge, ring, gear - so each item reads at a
// glance without the label.
const ITEMS: NavItem[] = [
  { id: 'vpn', label: 'VPN', icon: ShieldEnergyIcon },
  { id: 'network', label: 'Network', icon: DashboardSpeed02Icon },
  { id: 'usage', label: 'Usage', icon: ChartRingIcon },
  { id: 'settings', label: 'Settings', icon: Settings01Icon },
]

interface NavTabsProps {
  active: ViewId
  onChange: (view: ViewId) => void
  className?: string
}

export function NavTabs({ active, onChange, className }: NavTabsProps) {
  return (
    <nav aria-label="Main" className={cn('flex items-stretch', className)}>
      {ITEMS.map((item) => {
        const selected = item.id === active
        return (
          <button
            key={item.id}
            type="button"
            onClick={() => onChange(item.id)}
            aria-current={selected ? 'page' : undefined}
            className={cn(
              'group relative flex w-18 flex-col items-center justify-center gap-1 outline-none',
              'transition-colors duration-200 focus-visible:bg-tint/6',
              selected ? 'text-foreground' : 'text-muted-foreground hover:text-foreground/85',
            )}
          >
            <HugeiconsIcon icon={item.icon} size={19} strokeWidth={selected ? 2 : 1.7} />
            <span className={cn('text-[11px] leading-none', selected ? 'font-semibold' : 'font-medium')}>
              {item.label}
            </span>

            {/* One shared underline slides between tabs instead of fading per tab. */}
            {selected && (
              <motion.span
                layoutId="nav-underline"
                aria-hidden
                className="absolute inset-x-4 bottom-0 h-0.5 rounded-full bg-primary"
                transition={spring.soft}
              />
            )}
          </button>
        )
      })}
    </nav>
  )
}
