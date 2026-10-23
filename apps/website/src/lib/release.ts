// The latest OpenWire release, read from GitHub's public API on the server and cached for an
// hour, so every "Download" button points straight at the newest installer without a redeploy.
// If GitHub cannot be reached, the buttons fall back to the latest-release page.
import { cacheLife } from 'next/cache';
import { site } from '@/data/site';
import { formatBytes } from './format';

export type Asset = { name: string; size: number; sizeLabel: string; url: string };

export type Release = {
  version: string;
  tag: string;
  url: string;
  date: string;
  dateLabel: string;
  /** The first "What's new" heading, e.g. "Connection logs". */
  headline: string | null;
  /** The "What's new" section of the notes, as markdown. */
  notes: string;
  installer: Asset | null;
  msi: Asset | null;
  checksums: Asset | null;
  /** Whether this came from GitHub, or is the offline fallback. */
  live: boolean;
};

const API = 'https://api.github.com/repos/Imesh-Madushan-Dev/OpenWire/releases/latest';

const FALLBACK: Release = {
  version: '0.2.0',
  tag: 'v0.2.0',
  url: site.latest,
  date: '2026-10-22T20:56:33Z',
  dateLabel: formatDate('2026-10-22T20:56:33Z'),
  headline: null,
  notes: '',
  installer: null,
  msi: null,
  checksums: null,
  live: false,
};

type GitHubAsset = { name: string; size: number; browser_download_url: string };
type GitHubRelease = { tag_name: string; html_url: string; published_at: string; body: string | null; assets: GitHubAsset[] };

export async function getLatestRelease(): Promise<Release> {
  'use cache';
  cacheLife('hours');

  try {
    const response = await fetch(API, {
      headers: {
        Accept: 'application/vnd.github+json',
        'X-GitHub-Api-Version': '2022-11-28',
        ...(process.env.GITHUB_TOKEN ? { Authorization: `Bearer ${process.env.GITHUB_TOKEN}` } : {}),
      },
    });
    if (!response.ok) return FALLBACK;
    const data = (await response.json()) as GitHubRelease;
    const find = (test: (name: string) => boolean) => {
      const asset = data.assets.find((a) => test(a.name));
      return asset
        ? { name: asset.name, size: asset.size, sizeLabel: formatBytes(asset.size), url: asset.browser_download_url }
        : null;
    };
    const notes = whatsNew(data.body ?? '');
    return {
      version: data.tag_name.replace(/^v/, ''),
      tag: data.tag_name,
      url: data.html_url,
      date: data.published_at,
      dateLabel: formatDate(data.published_at),
      headline: notes.match(/^###\s+(.+)$/m)?.[1]?.trim() ?? null,
      notes,
      installer: find((n) => /setup\.exe$/i.test(n)),
      msi: find((n) => /\.msi$/i.test(n)),
      checksums: find((n) => /sha256/i.test(n)),
      live: true,
    };
  } catch {
    return FALLBACK;
  }
}

/** The section between "## What's new" and the next "## " heading. */
function whatsNew(body: string) {
  const text = body.replace(/\r\n/g, '\n');
  const start = text.search(/^##\s+What.s new\s*$/im);
  if (start < 0) return '';
  const rest = text.slice(start).split('\n').slice(1).join('\n');
  const end = rest.search(/^##\s/m);
  return (end < 0 ? rest : rest.slice(0, end)).trim();
}

function formatDate(iso: string) {
  return new Intl.DateTimeFormat('en-GB', { day: 'numeric', month: 'long', year: 'numeric', timeZone: 'UTC' }).format(
    new Date(iso),
  );
}
