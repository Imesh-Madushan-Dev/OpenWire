import { ArrowRight02Icon, CheckmarkCircle02Icon } from '@hugeicons/core-free-icons';

import { hero } from '@/data/site';
import { delay } from '@/lib/enter';
import type { Release } from '@/lib/release';
import { Icon } from '../icon';
import { DownloadButton, RepoButton } from '../pills';
import { PowerOrb, TunnelStatus } from '../power-orb';
import { ProductShot } from '../product-shot';

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

        <div className="hero__orb">
          <TunnelStatus />
          <div className="orb-slot" data-orb-slot>
            <div className="orb-flyer" data-orb-flyer>
              <PowerOrb />
            </div>
          </div>
          <p className="hero__caption">{hero.orbCaption}</p>
        </div>
      </div>

      <div className="frame">
        <ProductShot
          className="window--hero"
          src="/screens/vpn.webp"
          alt={hero.windowAlt}
          sizes="(min-width: 1400px) 1320px, calc(100vw - 40px)"
          orbTarget
          reveal
        />
      </div>
    </section>
  );
}
