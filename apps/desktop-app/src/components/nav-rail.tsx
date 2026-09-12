/** Left navigation rail. */

import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/react'
import {
  ChartLineData01Icon,
  GlobalIcon,
  PieChart01Icon,
  Settings02Icon,
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

const PRIMARY: NavItem[] = [
  { id: 'vpn', label: 'VPN', icon: GlobalIcon },
  { id: 'network', label: 'Network', icon: ChartLineData01Icon },
  { id: 'usage', label: 'Usage', icon: PieChart01Icon },
]

const SECONDARY: NavItem[] = [{ id: 'settings', label: 'Settings', icon: Settings02Icon }]

interface NavRailProps {
  active: ViewId
  onChange: (view: ViewId) => void
  /** Drives the status pip under the logo. */
  connected: boolean
}

export function NavRail({ active, onChange, connected }: NavRailProps) {
  return (
    <nav
      aria-label="Main"
      className="flex w-[84px] shrink-0 flex-col items-center gap-1 border-r border-white/6 bg-rail py-3"
    >
      <div className="relative mb-3 grid size-9 place-items-center rounded-xl bg-primary/12">
        <span aria-hidden className="text-sm font-bold text-primary">
          M
        </span>
        <motion.span
          aria-hidden
          className={cn(
            'absolute -right-0.5 -bottom-0.5 size-2.5 rounded-full border-2 border-rail',
            connected ? 'bg-primary' : 'bg-muted-foreground/40',
          )}
          animate={{ scale: connected ? [1, 1.25, 1] : 1 }}
          transition={
            connected
              ? { duration: 2.2, repeat: Infinity, ease: 'easeInOut' }
              : { duration: 0.2 }
          }
        />
      </div>

      {PRIMARY.map((item) => (
        <RailButton
          key={item.id}
          item={item}
          active={active === item.id}
          onClick={() => onChange(item.id)}
        />
      ))}

      <div className="flex-1" />

      {SECONDARY.map((item) => (
        <RailButton
          key={item.id}
          item={item}
          active={active === item.id}
          onClick={() => onChange(item.id)}
        />
      ))}
    </nav>
  )
}

function RailButton({
  item,
  active,
  onClick,
}: {
  item: NavItem
  active: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={active ? 'page' : undefined}
      className="group relative flex w-[68px] flex-col items-center gap-1 rounded-2xl py-2.5 outline-none focus-visible:ring-2 focus-visible:ring-primary/50"
    >
      {/* One shared element slides between items instead of fading per button. */}
      {active && (
        <motion.span
          layoutId="rail-active"
          aria-hidden
          className="absolute inset-0 rounded-2xl bg-white/7"
          transition={spring.soft}
        />
      )}

      <HugeiconsIcon
        icon={item.icon}
        size={20}
        strokeWidth={1.8}
        className={cn(
          'relative transition-colors duration-200',
          active ? 'text-primary' : 'text-muted-foreground group-hover:text-foreground/80',
        )}
      />
      <span
        className={cn(
          'relative text-[10px] font-medium transition-colors duration-200',
          active ? 'text-foreground' : 'text-muted-foreground group-hover:text-foreground/80',
        )}
      >
        {item.label}
      </span>
    </button>
  )
}
