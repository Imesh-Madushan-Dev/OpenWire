import { Tick02Icon } from '@hugeicons/core-free-icons';

import { features } from '@/data/site';
import { Icon } from '../icon';
import { ProductShot } from '../product-shot';
import { ScreenStage, TAB_ICONS, tone } from '../screen-stage';

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
                sizes="(max-width: 600px) 200vw, calc(100vw - 40px)"
              />
            </article>
          ))}
        </div>

        <ScreenStage className="chapters__stage" sizes="(min-width: 1400px) 760px, 56vw" jump />
      </div>
    </section>
  );
}
