/** Section list for the settings page. */

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
    // Sticky inside the page's scroller, so the list stays put on long sections.
    <nav aria-label="Settings sections" className="sticky top-4 flex w-44 shrink-0 flex-col self-start">
      <h1 className="px-3 pb-4 font-heading text-xl font-semibold">Settings</h1>

      <div role="tablist" aria-orientation="vertical" className="flex flex-col gap-0.5">
        {SECTION_GROUPS.map((group, index) => (
          <Fragment key={group[0].id}>
            {index > 0 && <span aria-hidden className="mx-3 my-2 h-px bg-hairline" />}
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
      className="group relative flex items-center gap-2.5 rounded-xl px-2 py-1.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-primary/50"
    >
      {/* One shared element slides between items instead of fading per button. */}
      {active && (
        <motion.span
          layoutId="settings-nav-active"
          aria-hidden
          className="absolute inset-0 rounded-xl bg-tint/7"
          transition={spring.soft}
        />
      )}

      {/* The section's colour, dimmed until it is the one open. */}
      <span
        className={cn(
          'tone-tile relative grid size-7 shrink-0 place-items-center rounded-md transition-opacity duration-200',
          active ? 'opacity-100' : 'opacity-70 group-hover:opacity-100',
        )}
        style={toneStyle(section.tone)}
      >
        <HugeiconsIcon icon={section.icon} size={15} strokeWidth={1.9} />
      </span>
      <span
        className={cn(
          'relative truncate text-sm font-medium transition-colors duration-200',
          active ? 'text-foreground' : 'text-muted-foreground group-hover:text-foreground/80',
        )}
      >
        {section.label}
      </span>
    </button>
  )
}
