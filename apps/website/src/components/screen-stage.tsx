import Image from 'next/image';
import type { CSSProperties } from 'react';
import {
  ChartLineIcon,
  Globe02Icon,
  MonitorSmartphoneIcon,
  PieChartIcon,
  Settings02Icon,
} from '@hugeicons/core-free-icons';
import type { IconSvgElement } from '@hugeicons/react';

import { features, type Chapter } from '@/data/site';
import { Icon } from './icon';
import { ScreenViewer } from './screen-viewer';

/** The app's own tab icons (apps/desktop-app/src/components/nav-tabs.tsx). */
export const TAB_ICONS: Record<string, IconSvgElement> = {
  vpn: Globe02Icon,
  network: ChartLineIcon,
  usage: PieChartIcon,
  devices: MonitorSmartphoneIcon,
  themes: Settings02Icon,
};

export const tone = (chapter: Chapter) => ({ '--tone': `var(--tone-${chapter.tone})` }) as CSSProperties;

/**
 * One window holding every tab's screen, with the app's tab bar under it. src/motion/sections.ts
 * swaps the screen and slides the marker: Features follows the scroll, the hero follows a clock.
 * With `jump`, the tabs link to their chapters; without it they are buttons that switch in place.
 */
export function ScreenStage({
  className = '',
  sizes,
  jump = false,
  eager = false,
}: {
  className?: string;
  sizes: string;
  jump?: boolean;
  /** The hero's window is above the fold: load the first screen at once. */
  eager?: boolean;
}) {
  const { chapters } = features;
  return (
    <div className={`stage ${className}`} style={tone(chapters[0])}>
      <figure className="window">
        <div className="window__bezel">
          <div className="window__screen">
            {chapters.map((chapter, i) => (
              <Image
                key={chapter.id}
                className="stage__shot"
                data-shot={i}
                data-active={i === 0 ? '' : undefined}
                src={chapter.shot.src}
                alt={chapter.shot.alt}
                width={1640}
                height={1040}
                quality={90}
                sizes={sizes}
                loading={eager && i === 0 ? 'eager' : 'lazy'}
                fetchPriority={eager && i === 0 ? 'high' : undefined}
                data-full={chapter.shot.src}
              />
            ))}
            <ScreenViewer />
          </div>
        </div>
      </figure>
      <nav className="tabs" aria-label={jump ? 'Jump to a tab' : 'Show a tab'} data-tabs>
        <span className="tabs__marker" aria-hidden="true" data-tabs-marker />
        {chapters.map((chapter, i) => {
          const props = {
            className: 'tabs__item',
            'data-tab': i,
            style: tone(chapter),
            'aria-current': i === 0 ? ('true' as const) : undefined,
          };
          const label = (
            <>
              <Icon icon={TAB_ICONS[chapter.id]} />
              <span className="tabs__label">{chapter.tab}</span>
            </>
          );
          return jump ? (
            <a key={chapter.id} href={`#chapter-${chapter.id}`} {...props}>
              {label}
            </a>
          ) : (
            <button key={chapter.id} type="button" {...props}>
              {label}
            </button>
          );
        })}
      </nav>
    </div>
  );
}
