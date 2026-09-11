/**
 * One ranked column of the usage breakdown.
 *
 * Rank is encoded by bar length and by a single accent at decreasing opacity -
 * a per-row rainbow would read as categorical meaning that is not there.
 */

import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/react'
import { motion } from 'motion/react'

import { formatBytes } from '@/lib/format'
import { staggerChild, staggerParent, transition } from '@/lib/motion'
import type { UsageEntry } from '@/lib/types'

interface BreakdownColumnProps {
  title: string
  icon: IconSvgElement
  entries: UsageEntry[]
  /** Shown in place of the list when there is nothing to rank. */
  emptyHint: string
}

export function BreakdownColumn({ title, icon, entries, emptyHint }: BreakdownColumnProps) {
  // Scale bars against the leader, not the column total, so the shape of the
  // ranking stays readable when one entry dominates.
  const leader = entries[0]?.total ?? 0

  return (
    <motion.section
      variants={staggerChild}
      className="panel flex min-w-0 flex-col gap-3 rounded-2xl p-4"
    >
      <h3 className="flex items-center gap-2 text-xs font-semibold tracking-wide text-muted-foreground uppercase">
        <HugeiconsIcon icon={icon} size={13} strokeWidth={2} />
        {title}
      </h3>

      {entries.length === 0 ? (
        <p className="py-6 text-center text-xs leading-relaxed text-muted-foreground">
          {emptyHint}
        </p>
      ) : (
        <motion.ol
          variants={staggerParent}
          initial="initial"
          animate="animate"
          className="flex flex-col gap-2.5"
        >
          {entries.map((entry, index) => (
            <motion.li
              key={entry.key}
              variants={staggerChild}
              layout
              className="flex min-w-0 flex-col gap-1.5"
            >
              <div className="flex min-w-0 items-baseline gap-2">
                <span className="truncate text-xs font-medium" title={entry.key}>
                  {entry.key}
                </span>
                <span className="numeric ml-auto shrink-0 text-[11px] text-muted-foreground">
                  {formatBytes(entry.total)}
                </span>
              </div>

              <div className="h-1 w-full overflow-hidden rounded-full bg-white/6">
                <motion.div
                  className="h-full rounded-full bg-primary"
                  // Fade with rank so the eye reads the order, not six colours.
                  style={{ opacity: Math.max(0.28, 1 - index * 0.09) }}
                  initial={{ width: 0 }}
                  animate={{
                    width: leader > 0 ? `${Math.max(2, (entry.total / leader) * 100)}%` : '0%',
                  }}
                  transition={transition.base}
                />
              </div>
            </motion.li>
          ))}
        </motion.ol>
      )}
    </motion.section>
  )
}
