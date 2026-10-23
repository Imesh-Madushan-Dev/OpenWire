import type { Metadata, Viewport } from 'next';
import { Figtree } from 'next/font/google';

import { Motion } from '@/components/motion';
import { MotionProvider } from '@/components/motion-provider';
import { SiteFooter } from '@/components/site-footer';
import { SiteHeader } from '@/components/site-header';
import { site } from '@/data/site';
import { getLatestRelease } from '@/lib/release';
import { siteUrl } from '@/lib/site-url';
import './globals.css';

const figtree = Figtree({ subsets: ['latin'], variable: '--font-figtree', display: 'swap' });

export const metadata: Metadata = {
  metadataBase: new URL(siteUrl),
  title: { default: site.title, template: `%s | ${site.name}` },
  description: site.description,
  applicationName: site.name,
  authors: [{ name: site.author, url: 'https://github.com/Imesh-Madushan-Dev' }],
  alternates: { canonical: './' },
  openGraph: {
    type: 'website',
    siteName: site.name,
    title: site.title,
    description: site.description,
    images: [{ url: '/og.png', width: 1200, height: 630, alt: 'OpenWire: see every byte, tunnel it yourself' }],
  },
  twitter: { card: 'summary_large_image', title: site.title, description: site.description, images: ['/og.png'] },
};

export const viewport: Viewport = {
  themeColor: '#07080d',
  colorScheme: 'dark',
};

// Before first paint: hidden-until-revealed styles apply only when JS and motion are on, and the
// room starts dark so the tunnel can open as you arrive. Without either, it renders lit.
const flags = `(function(){var d=document.documentElement;d.classList.add('js');if(!matchMedia('(prefers-reduced-motion: reduce)').matches){d.classList.add('motion');d.style.setProperty('--tunnel','0')}})()`;

export default async function RootLayout({ children }: LayoutProps<'/'>) {
  const release = await getLatestRelease();
  const download = {
    href: release.installer?.url ?? release.url,
    version: release.version,
    size: release.installer?.sizeLabel ?? null,
  };

  return (
    <html lang="en" className={`dark ${figtree.variable}`} suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: flags }} />
      </head>
      <body>
        <a className="skip-link" href="#main">
          Skip to content
        </a>
        <div className="room" aria-hidden="true" />
        <div id="curtain" aria-hidden="true">
          <div className="curtain__panel" />
          <svg className="curtain__ring">
            <circle cx="0" cy="0" r="0" />
          </svg>
          <div className="curtain__label">
            <span data-curtain-label />
          </div>
        </div>
        <MotionProvider>
          <SiteHeader download={download} />
          <main id="main" tabIndex={-1}>
            {children}
          </main>
          <SiteFooter />
        </MotionProvider>
        <Motion />
      </body>
    </html>
  );
}
