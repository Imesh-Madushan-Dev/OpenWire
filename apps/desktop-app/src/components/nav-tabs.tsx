/**
 * Main navigation: icon-over-label tabs centred in the title bar.
 *
 * The active tab is told apart by brightness first - a lit label against
 * muted ones - then by its icon taking the theme's accent and the underline,
 * so it never rests on colour alone.
 */

import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/react'
import {
  ChartRingIcon,
  DashboardSpeed02Icon,
  LaptopPhoneSyncIcon,
  Settings01Icon,
  ShieldEnergyIcon,
} from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'
import { cn } from 'cn'

import { spring } from '@/lib/motion'

export type ViewId = 'vpn' | 'network' | 'usage' | 'devices' | 'settings'

interface NavItem {
  id: ViewId
  label: string
  icon: IconSvgElement
}

// Distinct silhouettes - shield, gauge, ring, screens, gear - so each item
// reads at a glance without the label.
const ITEMS: NavItem[] = [
  { id: 'vpn', label: 'VPN', icon: ShieldEnergyIcon },
  { id: 'network', label: 'Network', icon: DashboardSpeed02Icon },
  { id: 'usage', label: 'Usage', icon: ChartRingIcon },
  { id: 'devices', label: 'Devices', icon: LaptopPhoneSyncIcon },
  { id: 'settings', label: 'Settings', icon: Settings01Icon },
]

interface NavTabsProps {
  active: ViewId
  onChange: (view: ViewId) => void
  /** Counts worth a glance from any tab, e.g. devices that just joined. */
  badges?: Partial<Record<ViewId, number>>
  className?: string
}

export function NavTabs({ active, onChange, badges, className }: NavTabsProps) {
  return (
    <nav aria-label="Main" className={cn('flex items-stretch', className)}>
      {ITEMS.map((item) => {
        const selected = item.id === active
        const badge = badges?.[item.id] ?? 0
        return (
          <button
            key={item.id}
            type="button"
            onClick={() => onChange(item.id)}
            aria-current={selected ? 'page' : undefined}
            aria-label={badge > 0 ? `${item.label}, ${badge} new` : undefined}
            className={cn(
              'group relative flex w-18 flex-col items-center justify-center gap-1 outline-none',
              'transition-colors duration-200 focus-visible:bg-tint/6',
              selected ? 'text-foreground' : 'text-muted-foreground hover:text-foreground/85',
            )}
          >
            <span className="relative flex">
              <HugeiconsIcon
                icon={item.icon}
                size={19}
                strokeWidth={selected ? 2 : 1.7}
                className={cn('transition-colors duration-200', selected && 'text-primary')}
              />
              {badge > 0 && (
                <span
                  aria-hidden
                  className="numeric action-fill absolute -top-1.5 left-3.5 grid h-3.5 min-w-3.5 place-items-center rounded-full bg-primary px-1 text-[9px] leading-none font-semibold text-primary-foreground ring-2 ring-background"
                >
                  {badge > 99 ? '99+' : badge}
                </span>
              )}
            </span>
            <span className={cn('text-[11px] leading-none', selected ? 'font-semibold' : 'font-medium')}>
              {item.label}
            </span>

            {/* One shared underline slides between tabs instead of fading per tab. */}
            {selected && (
              <motion.span
                layoutId="nav-underline"
                aria-hidden
                className="action-fill absolute inset-x-4 bottom-0 h-0.5 rounded-full bg-primary"
                transition={spring.soft}
              />
            )}
          </button>
        )
      })}
    </nav>
  )
}
