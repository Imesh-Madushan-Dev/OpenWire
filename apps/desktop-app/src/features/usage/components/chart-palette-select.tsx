/** A compact chart-palette menu for sitting beside a graph's other controls. */

import { useState } from 'react'
import { cn } from 'cn'

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import {
  CHART_PALETTES,
  readChartPalette,
  saveChartPalette,
  type ChartPaletteId,
} from '@/lib/chart-palette'

export function ChartPaletteSelect({ className }: { className?: string }) {
  const [palette, setPalette] = useState<ChartPaletteId>(readChartPalette)

  const choose = (id: ChartPaletteId | null) => {
    if (!id) return
    setPalette(id)
    saveChartPalette(id)
  }

  return (
    <Select value={palette} onValueChange={choose}>
      <SelectTrigger size="sm" aria-label="Chart colours" className={className}>
        <SelectValue>
          {(id: ChartPaletteId) => (
            <>
              <PaletteSwatch id={id} />
              <span className="text-xs">
                {CHART_PALETTES.find((option) => option.id === id)?.label}
              </span>
            </>
          )}
        </SelectValue>
      </SelectTrigger>
      <SelectContent align="end" alignItemWithTrigger={false}>
        {CHART_PALETTES.map((option) => (
          <SelectItem key={option.id} value={option.id}>
            <PaletteSwatch id={option.id} className="self-center" />
            <span className="text-xs">{option.label}</span>
            <span className="text-xs text-muted-foreground">{option.description}</span>
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}

/** The palette's two colours, download first, in whichever mode is active. */
function PaletteSwatch({ id, className }: { id: ChartPaletteId; className?: string }) {
  return (
    <span aria-hidden data-chart={id} className={cn('flex shrink-0 gap-0.5', className)}>
      <span className="h-2 w-3 rounded-full bg-chart-down" />
      <span className="h-2 w-3 rounded-full bg-chart-up" />
    </span>
  )
}
