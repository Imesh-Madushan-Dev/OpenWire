/**
 * Chart colour choice.
 *
 * Each preview carries its own `data-chart`, so it is drawn with that
 * palette's real tokens for the active mode - the same two colours the
 * Network and Usage charts will use.
 */

import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Tick02Icon } from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'
import { cn } from 'cn'

import {
  CHART_PALETTES,
  readChartPalette,
  saveChartPalette,
  type ChartPaletteId,
} from '@/lib/chart-palette'
import { staggerChild } from '@/lib/motion'

export function ChartPalettePicker() {
  const [current, setCurrent] = useState<ChartPaletteId>(readChartPalette)

  const choose = (id: ChartPaletteId) => {
    setCurrent(id)
    saveChartPalette(id)
  }

  return (
    <section className="flex flex-col gap-2">
      <h2 className="px-1 text-xs font-semibold tracking-wide text-muted-foreground uppercase">
        Chart colours
      </h2>
      <div role="radiogroup" aria-label="Chart colours" className="grid grid-cols-5 gap-2">
        {CHART_PALETTES.map((palette) => {
          const selected = palette.id === current
          return (
            <motion.button
              key={palette.id}
              variants={staggerChild}
              type="button"
              role="radio"
              aria-checked={selected}
              onClick={() => choose(palette.id)}
              className={cn(
                'panel-interactive flex flex-col gap-2 rounded-lg p-1.5 text-left outline-none',
                'focus-visible:ring-2 focus-visible:ring-primary/50',
                selected && 'border-primary/60',
              )}
            >
              <PalettePreview id={palette.id} />
              <span className="flex items-start gap-2 px-1.5 pb-1">
                <span className="min-w-0 flex-1">
                  <span className="block text-[13px] font-medium">{palette.label}</span>
                  <span className="block text-xs leading-relaxed text-muted-foreground">
                    {palette.description}
                  </span>
                </span>
                {selected && (
                  <span className="action-fill mt-0.5 grid size-4.5 shrink-0 place-items-center rounded-full bg-primary text-primary-foreground">
                    <HugeiconsIcon icon={Tick02Icon} size={11} strokeWidth={2.8} />
                  </span>
                )}
              </span>
            </motion.button>
          )
        })}
      </div>
    </section>
  )
}

// Two made-up traffic curves on a 120x48 canvas; download is the taller one.
const DOWN_LINE = 'M0 34 C10 30 16 14 26 16 C36 18 40 32 50 30 C60 28 64 8 76 10 C88 12 92 28 102 26 C110 24 114 18 120 16'
const UP_LINE = 'M0 42 C12 40 18 30 28 32 C38 34 44 40 56 38 C66 36 72 24 82 28 C92 32 100 40 120 36'
const toArea = (line: string) => `${line} L120 48 L0 48 Z`

/** A miniature throughput graph, drawn the way the real one is. */
function PalettePreview({ id }: { id: ChartPaletteId }) {
  return (
    <span
      aria-hidden
      data-chart={id}
      className="block h-16 overflow-hidden rounded-md border border-hairline bg-background px-2 pt-2.5"
    >
      <svg viewBox="0 0 120 48" preserveAspectRatio="none" className="size-full overflow-visible">
        <path d={toArea(DOWN_LINE)} fill="var(--chart-down)" fillOpacity={0.25} />
        <path d={toArea(UP_LINE)} fill="var(--chart-up)" fillOpacity={0.3} />
        <path
          d={DOWN_LINE}
          fill="none"
          stroke="var(--chart-down)"
          strokeWidth={1.5}
          vectorEffect="non-scaling-stroke"
        />
        <path
          d={UP_LINE}
          fill="none"
          stroke="var(--chart-up)"
          strokeWidth={1.5}
          vectorEffect="non-scaling-stroke"
        />
      </svg>
    </span>
  )
}
