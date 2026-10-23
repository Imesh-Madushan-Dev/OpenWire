import { ArrowUpRight01Icon, Download04Icon, GithubIcon, WindowsNewIcon } from '@hugeicons/core-free-icons';
import type { IconSvgElement } from '@hugeicons/react';

import { hero, site } from '@/data/site';
import type { Release } from '@/lib/release';
import { Icon } from './icon';

type Size = 'sm' | 'md' | 'lg';
const sizeClass = (size: Size) => (size === 'md' ? '' : ` pill--${size}`);

/** A label that rolls up on hover; the copy underneath is for sighted pointer users only. */
export function Roll({ text }: { text: string }) {
  return (
    <span className="roll">
      <span>{text}</span>
      <span aria-hidden="true">{text}</span>
    </span>
  );
}

/** The direct download of the newest installer, straight from the GitHub release. */
export function DownloadButton({
  release,
  size = 'lg',
  label = hero.download,
  meta = true,
}: {
  release: Release;
  size?: Size;
  label?: string;
  meta?: boolean;
}) {
  const asset = release.installer;
  return (
    <a className={`pill pill--primary${sizeClass(size)}`} href={asset?.url ?? release.url}>
      <span className="swap" aria-hidden="true">
        <Icon icon={WindowsNewIcon} />
        <Icon icon={Download04Icon} />
      </span>
      <Roll text={label} />
      {meta && asset && (
        <span className="pill__meta">
          <span>{release.version}</span>
          <span>{asset.sizeLabel}</span>
        </span>
      )}
    </a>
  );
}

/** An outline pill that floods with ink from the pointer's side (src/motion/fill.ts). */
export function GhostButton({
  href,
  label,
  icon,
  size = 'lg',
  external = true,
}: {
  href: string;
  label: string;
  icon?: IconSvgElement;
  size?: Size;
  external?: boolean;
}) {
  return (
    <a
      className={`pill pill--ghost${sizeClass(size)}`}
      href={href}
      data-fill
      {...(external ? { target: '_blank', rel: 'noopener noreferrer' } : {})}
    >
      <span className="pill__fill" aria-hidden="true" />
      {icon && <Icon icon={icon} />}
      <span>{label}</span>
      {external && (
        <span className="pill__nudge" aria-hidden="true">
          <Icon icon={ArrowUpRight01Icon} size={18} />
        </span>
      )}
      {external && <span className="sr-only">(opens in a new tab)</span>}
    </a>
  );
}

export function RepoButton({ size = 'lg', label = hero.repo }: { size?: Size; label?: string }) {
  return <GhostButton href={site.repo} label={label} icon={GithubIcon} size={size} />;
}
