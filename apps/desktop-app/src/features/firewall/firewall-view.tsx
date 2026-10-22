/**
 * Firewall: a placeholder until the feature exists. It keeps the page's
 * toolbar band, so the chrome does not jump when switching to this tab.
 */

import { HugeiconsIcon } from '@hugeicons/react'
import { FireIcon } from '@hugeicons/core-free-icons'

import { Toolbar } from '@/components/toolbar'
import { toneStyle } from '@/lib/tone'

export function FirewallView() {
  return (
    <div className="flex h-full flex-col overflow-hidden">
      <Toolbar />

      <div className="grid min-h-0 flex-1 place-items-center px-10">
        <div className="flex flex-col items-center gap-3 text-center">
          <span
            className="tone-tile grid size-12 place-items-center rounded-lg"
            style={toneStyle('rose')}
          >
            <HugeiconsIcon icon={FireIcon} size={24} strokeWidth={1.8} />
          </span>
          <h1 className="text-sm font-semibold">Firewall</h1>
          <p className="text-xs text-muted-foreground">Coming soon.</p>
        </div>
      </div>
    </div>
  )
}
