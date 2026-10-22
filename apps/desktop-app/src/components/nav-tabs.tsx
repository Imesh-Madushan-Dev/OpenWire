/**
 * Main navigation: GlassWire's big icon-over-label tabs, centred in the title
 * band, each glyph the Hugeicons shape nearest to GlassWire's own.
 *
 * The active tab's icon takes the accent and an underline runs along the
 * band's bottom edge beneath it - two signals, so it never rests on colour
 * alone. Labels stay equally bright either way, as GlassWire's do; only their
 * weight changes.
 */

import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/react'
import {
  ChartLineIcon,
  FireIcon,
  Globe02Icon,
  MonitorSmartphoneIcon,
  PieChartIcon,
  Settings02Icon,
} from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'
import { cn } from 'cn'

import { spring } from '@/lib/motion'

export type ViewId = 'vpn' | 'network' | 'usage' | 'devices' | 'firewall' | 'settings'

interface NavItem {
  id: ViewId
  label: string
  icon: IconSvgElement
}

// Distinct silhouettes - globe, line graph, pie, screens, flame, gear - so each
// tab reads at a glance without its label. The tunnel gets a globe rather
// than a shield or lock: Maizy disguises traffic, it does not promise
// protection.
const ITEMS: NavItem[] = [
  { id: 'vpn', label: 'VPN', icon: Globe02Icon },
  { id: 'network', label: 'Network', icon: ChartLineIcon },
  { id: 'usage', label: 'Usage', icon: PieChartIcon },
  { id: 'devices', label: 'Devices', icon: MonitorSmartphoneIcon },
  { id: 'firewall', label: 'Firewall', icon: FireIcon },
  { id: 'settings', label: 'Settings', icon: Settings02Icon },
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
              'group relative flex w-19 flex-col items-center justify-end gap-1.5 pb-3.5 outline-none',
              'transition-colors duration-200 focus-visible:bg-tint/6',
            )}
          >
            <span className="relative flex">
              <HugeiconsIcon
                icon={item.icon}
                size={28}
                strokeWidth={selected ? 2.1 : 1.9}
                className={cn(
                  'transition-colors duration-200',
                  selected ? 'text-primary' : 'text-muted-foreground group-hover:text-foreground/80',
                )}
              />
              {badge > 0 && (
                <span
                  aria-hidden
                  className="numeric action-fill absolute -top-1.5 -right-2.5 grid h-3.5 min-w-3.5 place-items-center rounded-full bg-primary px-1 text-[9px] leading-none font-semibold text-primary-foreground ring-2 ring-rail"
                >
                  {badge > 99 ? '99+' : badge}
                </span>
              )}
            </span>
            <span
              className={cn(
                'text-xs leading-none text-foreground/90',
                selected ? 'font-semibold' : 'font-medium',
              )}
            >
              {item.label}
            </span>

            {/* One shared underline slides between tabs instead of fading per tab. */}
            {selected && (
              <motion.span
                layoutId="nav-underline"
                aria-hidden
                className="absolute inset-x-0 -bottom-px h-0.5 bg-primary"
                transition={spring.soft}
              />
            )}
          </button>
        )
      })}
    </nav>
  )
}
