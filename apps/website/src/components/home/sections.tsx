import Link from 'next/link';
import type { CSSProperties } from 'react';
import { GithubIcon, Link01Icon, SourceCodeIcon, Wifi01Icon } from '@hugeicons/core-free-icons';

import { firstServer, openSource, privacy, site, statement } from '@/data/site';
import type { Release } from '@/lib/release';
import { Icon } from '../icon';
import { GhostButton } from '../pills';

/** The case for the product in one paragraph; its words light up as you read down (statement.ts). */
export function Statement() {
  return (
    <section className="statement" aria-labelledby="statement-title">
      <h2 id="statement-title" className="sr-only">
        Why OpenWire
      </h2>
      <div className="frame">
        <p className="statement__text" data-statement>
          {statement}
        </p>
      </div>
    </section>
  );
}

export function FirstServer() {
  return (
    <section className="section" aria-labelledby="byos-title">
      <div className="frame split">
        <div className="split__copy">
          <h2 id="byos-title" className="h-xl" data-split="lines">
            {firstServer.title}
          </h2>
          <p className="lede" data-reveal="up">
            {firstServer.lede}
          </p>
          <ol className="numbered" data-reveal="up" data-delay="0.1">
            {firstServer.steps.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ol>
          <p className="note" data-reveal="fade" data-delay="0.2">
            {firstServer.note}
          </p>
        </div>

        <figure className="split__side paste-demo" data-paste-demo data-reveal="up">
          <div className="paste">
            <span className="paste__icon" aria-hidden="true">
              <Icon icon={Link01Icon} />
            </span>
            <code className="paste__text" data-paste-text>
              {firstServer.example}
            </code>
          </div>
          <div className="keys" aria-hidden="true">
            <kbd data-key>Ctrl</kbd>
            <span>+</span>
            <kbd data-key>V</kbd>
          </div>
          <div className="added" data-added>
            <span className="tone-tile added__region" style={{ '--tone': 'var(--tone-sky)' } as CSSProperties}>
              JP
            </span>
            <span className="added__name">
              <span>{firstServer.added.name}</span>
              <span>{firstServer.added.host}</span>
            </span>
            <span className="added__ms">{firstServer.added.latency}</span>
          </div>
          <figcaption>{firstServer.exampleCaption}</figcaption>
        </figure>
      </div>
    </section>
  );
}

const TALKS_ICONS = [Wifi01Icon, GithubIcon];

export function PrivacySection() {
  return (
    <section className="section" aria-labelledby="privacy-title">
      <div className="frame split">
        <div className="split__copy">
          <h2 id="privacy-title" className="h-xl" data-split="lines">
            {privacy.title}
          </h2>
          <p className="lede" data-reveal="up">
            {privacy.lede}
          </p>
          <p data-reveal="fade">
            <Link className="link link--solo" href="/privacy">
              {privacy.more}
            </Link>
          </p>
        </div>
        <div className="split__side">
          <dl className="ledger">
            {privacy.rows.map((row, i) => (
              <div key={row.what} className="ledger__row" data-reveal="up" data-delay={String(i * 0.06)}>
                <dt>{row.what}</dt>
                <dd className={row.where.startsWith('%') ? 'mono' : undefined}>{row.where}</dd>
              </div>
            ))}
          </dl>
          <h3 className="label talks__title">Beyond your servers, it talks to</h3>
          <ul className="talks" data-reveal="up" data-delay="0.2">
            {privacy.talksTo.map((item, i) => (
              <li key={item.what}>
                <span className="tone-tile" aria-hidden="true">
                  <Icon icon={TALKS_ICONS[i]} />
                </span>
                <span>
                  <span>{item.what}</span>
                  <span>{item.when}</span>
                </span>
              </li>
            ))}
          </ul>
        </div>
      </div>
    </section>
  );
}

export function OpenSource({ release }: { release: Release }) {
  const facts = [
    ...(release.installer
      ? [{ value: release.installer.sizeLabel, label: `The whole ${release.version} installer` }]
      : []),
    ...openSource.facts,
  ];
  return (
    <section className="section" aria-labelledby="oss-title">
      <div className="frame">
        <div className="section-head">
          <h2 id="oss-title" className="h-xl" data-split="lines">
            {openSource.title}
          </h2>
          <p className="lede" style={{ maxWidth: '46ch' }} data-reveal="up">
            {openSource.lede}
          </p>
          <div className="actions" data-reveal="up" data-delay="0.1">
            <GhostButton href={site.repo} label={openSource.source} icon={SourceCodeIcon} />
            <GhostButton href={site.newIssue} label={openSource.issue} icon={GithubIcon} />
          </div>
        </div>

        <dl className="facts">
          {facts.map((fact, i) => (
            <div key={fact.value} className="fact" data-reveal="up" data-delay={String(i * 0.08)}>
              <dt>{fact.label}</dt>
              <dd>{fact.value}</dd>
            </div>
          ))}
        </dl>

        <div>
          <h3 className="h-l" data-split="lines">
            {openSource.roadmapTitle}
          </h3>
          <ul className="roadmap__list">
            {openSource.roadmap.map((item, i) => (
              <li key={item.name} className="roadmap__row" data-reveal="up" data-delay={String(i * 0.05)}>
                <h4>{item.name}</h4>
                <p>{item.text}</p>
                <span className="tag">Planned</span>
              </li>
            ))}
          </ul>
        </div>
      </div>
    </section>
  );
}
