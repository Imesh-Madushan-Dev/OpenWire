import { Features } from '@/components/home/features';
import { Hero } from '@/components/home/hero';
import { Route } from '@/components/home/route';
import { FirstServer, OpenSource, PrivacySection, Statement } from '@/components/home/sections';
import { site } from '@/data/site';
import { PageReady } from '@/engine/next';
import { getLatestRelease } from '@/lib/release';
import { siteUrl } from '@/lib/site-url';

export default async function Home() {
  const release = await getLatestRelease();

  const jsonLd = {
    '@context': 'https://schema.org',
    '@type': 'SoftwareApplication',
    name: site.name,
    description: site.description,
    url: siteUrl,
    applicationCategory: 'UtilitiesApplication',
    operatingSystem: 'Windows 10, Windows 11',
    softwareVersion: release.version,
    ...(release.installer ? { downloadUrl: release.installer.url, fileSize: release.installer.sizeLabel } : {}),
    offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
    license: 'https://www.gnu.org/licenses/gpl-3.0.html',
    author: { '@type': 'Person', name: site.author },
    sameAs: [site.repo],
  };

  return (
    <>
      <Hero release={release} />
      <Statement />
      <Features />
      <Route />
      <FirstServer />
      <PrivacySection />
      <OpenSource release={release} />
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd).replace(/</g, '\\u003c') }}
      />
      <PageReady />
    </>
  );
}
