import { ArrowRight02Icon, CheckmarkCircle02Icon } from '@hugeicons/core-free-icons';

import { hero } from '@/data/site';
import { delay } from '@/lib/enter';
import type { Release } from '@/lib/release';
import { Icon } from '../icon';
import { DownloadButton, RepoButton } from '../pills';
import { ScreenStage } from '../screen-stage';

export function Hero({ release }: { release: Release }) {
  const news = release.headline
    ? `New in ${release.version}: ${release.headline}`
    : `OpenWire ${release.version} is out`;

  return (
    <section className="hero" aria-labelledby="hero-title">
      <div className="frame hero__grid">
        <div className="hero__copy">
          <a
            className="chip"
            href={release.url}
            target="_blank"
            rel="noopener noreferrer"
            data-enter
            style={delay(0)}
          >
            <span className="chip__dot" aria-hidden="true" />
            <span>{news}</span>
            <Icon icon={ArrowRight02Icon} />
            <span className="sr-only">(release notes, opens in a new tab)</span>
          </a>

          <h1 id="hero-title" className="hero__title">
            <span className="rise" style={delay(0.08)}>
              <span>{hero.titleLead}</span>
            </span>{' '}
            <span className="rise" style={delay(0.17)}>
              <span className="hero__tunnel">{hero.titleTunnel}</span>
            </span>
          </h1>

          <p className="hero__lede" data-enter="slide" style={delay(0.3)}>
            {hero.lede}
          </p>

          <div className="actions hero__actions" data-enter="slide" style={delay(0.4)}>
            <DownloadButton release={release} />
            <RepoButton />
          </div>

          <ul className="hero__facts" data-enter style={delay(0.6)}>
            {hero.facts.map((fact) => (
              <li key={fact}>
                <Icon icon={CheckmarkCircle02Icon} />
                {fact}
              </li>
            ))}
          </ul>
        </div>

        <div className="hero__screens" data-hero-screens data-enter style={delay(0.45)}>
          <ScreenStage
            sizes="(min-width: 1400px) 660px, (min-width: 1024px) 50vw, calc(100vw - 40px)"
            eager
          />
        </div>
      </div>
    </section>
  );
}
