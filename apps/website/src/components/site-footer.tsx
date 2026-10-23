import Link from 'next/link';
import { ArrowUpRight01Icon } from '@hugeicons/core-free-icons';

import { colophon, finale, site } from '@/data/site';
import { getLatestRelease } from '@/lib/release';
import { Icon } from './icon';
import { DownloadButton, RepoButton } from './pills';
import { VisitMeter } from './visit-meter';

const product = [
  { label: 'Features', href: '/#features' },
  { label: 'How it works', href: '/#how' },
  { label: 'Download', href: '/download' },
  { label: 'Privacy', href: '/privacy' },
];

const project = [
  { label: 'Source code', href: site.repo },
  { label: 'All releases', href: site.releases },
  { label: 'Report a bug', href: site.issues },
  { label: 'License', href: site.license },
];

export async function SiteFooter() {
  const release = await getLatestRelease();
  return (
    <footer className="finale">
      <div className="frame finale__cta">
        <h2 className="finale__title" data-split="lines">
          <span>{finale.title}</span>
        </h2>
        <p className="lede" data-reveal="up">
          {finale.lede}
        </p>
        <div className="actions" data-reveal="up" data-delay="0.1">
          <DownloadButton release={release} />
          <RepoButton />
        </div>
      </div>

      <div className="frame finale__grid">
        <nav className="finale__nav" aria-label="Footer">
          <div>
            <h2 className="label">Product</h2>
            <ul>
              {product.map((item) => (
                <li key={item.href}>
                  <Link className="foot-link" href={item.href}>
                    {item.label}
                  </Link>
                </li>
              ))}
            </ul>
          </div>
          <div>
            <h2 className="label">On GitHub</h2>
            <ul>
              {project.map((item) => (
                <li key={item.href}>
                  <a className="foot-link" href={item.href} target="_blank" rel="noopener noreferrer">
                    {item.label}
                    <Icon icon={ArrowUpRight01Icon} />
                    <span className="sr-only">(opens in a new tab)</span>
                  </a>
                </li>
              ))}
            </ul>
          </div>
        </nav>
        <VisitMeter />
      </div>

      {/* The name as a logotype: drawn in SVG, outline only, so it reads as a mark, not as text. */}
      <div className="finale__mark" aria-hidden="true">
        <svg className="finale__word" viewBox="0 0 1000 190" data-reveal="up">
          <text x="500" y="152" textAnchor="middle" textLength="980" lengthAdjust="spacingAndGlyphs">
            OpenWire
          </text>
        </svg>
        <div className="finale__wire" data-wire />
      </div>

      <div className="frame finale__small">
        <p>
          © {site.year} {site.author}. Free software under the GPL-3.0.
        </p>
        <p>{colophon}</p>
      </div>
    </footer>
  );
}
