/**
 * Transient message strip.
 *
 * Errors stay until dismissed; informational messages clear themselves. An
 * error the user never saw is worse than one that lingers.
 */

import { useEffect } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Alert02Icon,
  Cancel01Icon,
  InformationCircleIcon,
} from '@hugeicons/core-free-icons'
import { AnimatePresence, motion } from 'motion/react'
import { cn } from 'cn'

import { transition } from '@/lib/motion'
import type { Notice } from '@/lib/types'

const AUTO_DISMISS_MS = 6000

const TONE = {
  info: 'border-tint/10 text-foreground',
  warning: 'border-warning/35 text-warning',
  error: 'border-destructive/40 text-destructive',
} as const

export function NoticeBar({
  notice,
  onDismiss,
}: {
  notice: Notice | null
  onDismiss: () => void
}) {
  const transient = notice?.level === 'info'

  useEffect(() => {
    if (!notice || !transient) return
    const timer = window.setTimeout(onDismiss, AUTO_DISMISS_MS)
    return () => window.clearTimeout(timer)
  }, [notice, transient, onDismiss])

  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-0 z-40 flex justify-center px-6 pb-6">
      <AnimatePresence>
        {notice && (
          <motion.div
            key={notice.message}
            role={notice.level === 'error' ? 'alert' : 'status'}
            initial={{ opacity: 0, y: 12, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 12, scale: 0.98 }}
            transition={transition.base}
            className={cn(
              'pointer-events-auto flex max-w-md items-start gap-2.5 rounded-2xl border bg-card/95 px-4 py-3 shadow-xl backdrop-blur',
              TONE[notice.level],
            )}
          >
            <HugeiconsIcon
              icon={notice.level === 'info' ? InformationCircleIcon : Alert02Icon}
              size={16}
              strokeWidth={2}
              className="mt-0.5 shrink-0"
            />
            <p className="flex-1 text-xs leading-relaxed">{notice.message}</p>
            <button
              type="button"
              onClick={onDismiss}
              aria-label="Dismiss"
              className="-mt-0.5 -mr-1 grid size-6 shrink-0 place-items-center rounded-lg text-current/70 transition-colors hover:bg-tint/8 hover:text-current focus-visible:ring-2 focus-visible:ring-primary/50 focus-visible:outline-none"
            >
              <HugeiconsIcon icon={Cancel01Icon} size={13} strokeWidth={2.2} />
            </button>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}
