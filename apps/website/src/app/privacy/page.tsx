import type { Metadata } from 'next';

import { privacy, site } from '@/data/site';
import { PageReady } from '@/engine/next';
import { delay } from '@/lib/enter';

export const metadata: Metadata = {
  title: 'Privacy',
  description:
    'OpenWire has no accounts, no analytics and no telemetry. What the app talks to, where your data lives on your PC, and what this website does.',
};

export default function PrivacyPage() {
  return (
    <>
      <section className="page-hero" aria-labelledby="privacy-title">
        <div className="frame page-hero__grid">
          <div className="page-hero__copy">
            <h1 id="privacy-title" className="page-hero__title">
              <span className="rise" style={delay(0.06)}>
                <span>Privacy</span>
              </span>
            </h1>
            <p className="lede" style={{ maxWidth: '46ch', ...delay(0.24) }} data-enter="slide">
              OpenWire has no accounts, no analytics and no telemetry. This page lists everything it talks to, and
              where everything it keeps is stored.
            </p>
          </div>
        </div>
      </section>

      <div className="frame doc">
        <section className="doc__block" aria-labelledby="talks-title">
          <h2 id="talks-title" data-split="lines">
            What the app talks to
          </h2>
          <div className="doc__body" data-reveal="up">
            <ul>
              <li>
                <strong>The servers you add.</strong> Your traffic goes to them while the tunnel is on, and OpenWire
                measures how quickly they answer.
              </li>
              <li>
                <strong>Devices on your own network</strong>, only while it scans them.
              </li>
              <li>
                <strong>GitHub, twice a day</strong>, to see whether a new release is out. The request carries nothing
                about you or your traffic, and you can turn it off in Settings › General.
              </li>
            </ul>
            <p>That is the whole list.</p>
          </div>
        </section>

        <section className="doc__block" aria-labelledby="where-title">
          <h2 id="where-title" data-split="lines">
            Where your data stays
          </h2>
          <div className="doc__body" data-reveal="up">
            <dl className="ledger">
              {privacy.rows.map((row) => (
                <div key={row.what} className="ledger__row">
                  <dt>{row.what}</dt>
                  <dd className={row.where.startsWith('%') ? 'mono' : undefined}>{row.where}</dd>
                </div>
              ))}
            </dl>
            <p>All of it is on your PC. Uninstalling OpenWire and deleting that folder removes it.</p>
          </div>
        </section>

        <section className="doc__block" aria-labelledby="monitoring-title">
          <h2 id="monitoring-title" data-split="lines">
            Detailed monitoring
          </h2>
          <div className="doc__body" data-reveal="up">
            <p>
              Turn it on to see every app on your PC, not only the traffic that goes through the tunnel. A small
              background service reads Windows’ own network events. There is no kernel driver, and it never reads the
              contents of your traffic.
            </p>
            <p>The installer offers it during setup. You can change it any time in Settings › Monitoring.</p>
          </div>
        </section>

        <section className="doc__block" aria-labelledby="country-title">
          <h2 id="country-title" data-split="lines">
            Countries and logs
          </h2>
          <div className="doc__body" data-reveal="up">
            <p>
              The country next to each host comes from an IP database built into the app, from{' '}
              <a className="link" href="https://db-ip.com">
                DB-IP
              </a>{' '}
              under CC BY 4.0, so lookups never leave your PC.
            </p>
            <p>
              Connection logs are kept in memory only, capped at 500 lines, and cleared when OpenWire closes. Server
              passwords and keys never appear in them.
            </p>
          </div>
        </section>

        <section className="doc__block" aria-labelledby="site-title">
          <h2 id="site-title" data-split="lines">
            This website
          </h2>
          <div className="doc__body" data-reveal="up">
            <p>
              This site sets no cookies and runs no analytics. The version number and download links come from
              GitHub’s public API, fetched by the site’s server, not your browser, and its fonts are served from here.
            </p>
            <p>
              The “this visit” figure in the footer is counted in your browser from the page’s own requests, and never
              leaves it.
            </p>
            <p>
              Questions or something that looks wrong?{' '}
              <a className="link" href={site.newIssue} target="_blank" rel="noopener noreferrer">
                Open an issue on GitHub
              </a>
              .
            </p>
          </div>
        </section>
      </div>
      <PageReady />
    </>
  );
}
