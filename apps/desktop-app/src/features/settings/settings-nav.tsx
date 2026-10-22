/**
 * Section list for the settings page: a full-height column of flat rows, the
 * open one lit and marked with an accent bar, as in GlassWire's own settings.
 */

import { Fragment } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { motion } from 'motion/react'
import { cn } from 'cn'

import { spring } from '@/lib/motion'
import { toneStyle } from '@/lib/tone'
import { SECTION_GROUPS, type Section, type SectionId } from './sections'

interface SettingsNavProps {
  active: SectionId
  onChange: (section: SectionId) => void
}

export function SettingsNav({ active, onChange }: SettingsNavProps) {
  return (
    <nav
      aria-label="Settings sections"
      className="flex w-48 shrink-0 flex-col overflow-y-auto border-r border-hairline bg-rail/40 py-2"
    >
      <div role="tablist" aria-orientation="vertical" className="flex flex-col">
        {SECTION_GROUPS.map((group, index) => (
          <Fragment key={group[0].id}>
            {index > 0 && <span aria-hidden className="mx-3 my-1.5 h-px bg-hairline" />}
            {group.map((section) => (
              <NavButton
                key={section.id}
                section={section}
                active={active === section.id}
                onClick={() => onChange(section.id)}
              />
            ))}
          </Fragment>
        ))}
      </div>
    </nav>
  )
}

function NavButton({
  section,
  active,
  onClick,
}: {
  section: Section
  active: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      role="tab"
      id={`settings-tab-${section.id}`}
      aria-selected={active}
      aria-controls="settings-panel"
      onClick={onClick}
      className={cn(
        'group relative flex h-8 items-center gap-2.5 px-3 text-left outline-none transition-colors duration-150',
        'focus-visible:ring-2 focus-visible:ring-primary/50 focus-visible:ring-inset',
        active ? 'bg-tint/9' : 'hover:bg-tint/5',
      )}
    >
      {/* One shared bar slides between items instead of fading per button. */}
      {active && (
        <motion.span
          layoutId="settings-nav-active"
          aria-hidden
          className="absolute inset-y-0 left-0 w-0.5 bg-primary"
          transition={spring.soft}
        />
      )}

      {/* The section's colour, dimmed until it is the one open. */}
      <span
        className={cn(
          'tone-tile grid size-5.5 shrink-0 place-items-center rounded-sm transition-opacity duration-200',
          active ? 'opacity-100' : 'opacity-75 group-hover:opacity-100',
        )}
        style={toneStyle(section.tone)}
      >
        <HugeiconsIcon icon={section.icon} size={13} strokeWidth={2} />
      </span>
      <span
        className={cn(
          'truncate text-[13px] transition-colors duration-150',
          active ? 'font-semibold text-foreground' : 'font-medium text-foreground/80 group-hover:text-foreground',
        )}
      >
        {section.label}
      </span>
    </button>
  )
}
