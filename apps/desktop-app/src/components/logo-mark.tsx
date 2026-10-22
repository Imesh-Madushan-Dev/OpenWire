/**
 * The app's logo (scripts/generate-logo.ts): an open "O" with a wire leaving
 * through the gap. The midnight tile in dark mode, the glass one in light,
 * matching the window and tray icons, which swap with the mode too.
 */

import { cn } from 'cn'

export function LogoMark({ size = 20, className }: { size?: number; className?: string }) {
  return (
    <>
      <img src="/logo-light.svg" alt="" width={size} height={size} className={cn('dark:hidden', className)} />
      <img src="/logo-dark.svg" alt="" width={size} height={size} className={cn('hidden dark:block', className)} />
    </>
  )
}
