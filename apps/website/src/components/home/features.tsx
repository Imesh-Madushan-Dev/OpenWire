import Image from 'next/image';
import type { CSSProperties } from 'react';
import {
  ChartLineIcon,
  Globe02Icon,
  MonitorSmartphoneIcon,
  PieChartIcon,
  Settings02Icon,
  Tick02Icon,
} from '@hugeicons/core-free-icons';
import type { IconSvgElement } from '@hugeicons/react';

import { features, type Chapter } from '@/data/site';
import { Icon } from '../icon';
import { ProductShot } from '../product-shot';

/** The app's own tab icons (apps/desktop-app/src/components/nav-tabs.tsx). */
const TAB_ICONS: Record<string, IconSvgElement> = {
  vpn: Globe02Icon,
  network: ChartLineIcon,
  usage: PieChartIcon,
  devices: MonitorSmartphoneIcon,
  themes: Settings02Icon,
};

const tone = (chapter: Chapter) => ({ '--tone': `var(--tone-${chapter.tone})` }) as CSSProperties;

/**
 * Five chapters beside one sticky window (src/motion/chapters.ts swaps the screen and moves the
 * tab marker). On phones and tablets each chapter carries its own screen instead.
 */
export function Features() {
  const { chapters } = features;
  return (
    <section id="features" className="section features" aria-labelledby="features-title">
      <div className="frame">
        <div className="section-head">
          <h2 id="features-title" className="h-xl" data-split="lines">
            {features.title}
          </h2>
          <p className="lede" data-reveal="up">
            {features.lede}
          </p>
        </div>
      </div>

      <div className="frame chapters" data-chapters>
        <div className="chapters__list">
          {chapters.map((chapter, i) => (
            <article
              key={chapter.id}
              id={`chapter-${chapter.id}`}
              className={`chapter${i === 0 ? ' is-active' : ''}`}
              data-chapter={i}
              style={tone(chapter)}
              aria-labelledby={`chapter-${chapter.id}-title`}
            >
              <div className="chapter__tab">
                <span className="tone-tile">
                  <Icon icon={TAB_ICONS[chapter.id]} />
                </span>
                <span>{chapter.tab}</span>
              </div>
              <h3 id={`chapter-${chapter.id}-title`} className="chapter__title">
                {chapter.title}
              </h3>
              <p className="chapter__body">{chapter.body}</p>
              {chapter.points && (
                <ul className="points">
                  {chapter.points.map((point) => (
                    <li key={point}>
                      <Icon icon={Tick02Icon} />
                      <span>{point}</span>
                    </li>
                  ))}
                </ul>
              )}
              <ProductShot
                className="chapter__shot"
                src={chapter.shot.src}
                alt={chapter.shot.alt}
                focus={chapter.shot.focus}
                sizes="(max-width: 600px) 200vw, calc(100vw - 40px)"
              />
            </article>
          ))}
        </div>

        <div className="chapters__stage">
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
                    sizes="(min-width: 1400px) 760px, 56vw"
                  />
                ))}
              </div>
            </div>
          </figure>
          <nav className="tabs" aria-label="Jump to a tab" data-tabs>
            <span className="tabs__marker" aria-hidden="true" data-tabs-marker />
            {chapters.map((chapter, i) => (
              <a
                key={chapter.id}
                className="tabs__item"
                href={`#chapter-${chapter.id}`}
                data-tab={i}
                style={tone(chapter)}
                aria-current={i === 0 ? 'true' : undefined}
              >
                <Icon icon={TAB_ICONS[chapter.id]} />
                {chapter.tab}
              </a>
            ))}
          </nav>
        </div>
      </div>
    </section>
  );
}
