import { HugeiconsIcon, type IconSvgElement } from '@hugeicons/react';

/** Hugeicons at the site's stroke: a touch lighter than the default, like the app's tabs. */
export function Icon({ icon, size = 20, className }: { icon: IconSvgElement; size?: number; className?: string }) {
  return <HugeiconsIcon icon={icon} size={size} strokeWidth={1.7} className={className} aria-hidden="true" />;
}
