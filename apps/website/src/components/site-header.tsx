'use client';

import Image from 'next/image';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { useEffect, useEffectEvent, useRef, useState, type CSSProperties } from 'react';
import { AnimatePresence } from 'motion/react';
import * as m from 'motion/react-m';
import { ArrowRight02Icon, Cancel01Icon, GithubIcon, Menu01Icon } from '@hugeicons/core-free-icons';

import { nav, site } from '@/data/site';
import { EASE } from '@/lib/motion';
import { useReducedMotion } from '@/lib/use-reduced-motion';
import { startScroll, stopScroll } from '@/engine';
import { toggle, type Phase } from '@/motion/tunnel';
import { Icon } from './icon';
import { useTunnelPhase } from './power-orb';
import { Roll, WindowsLogo } from './pills';

export type HeaderDownload = { href: string; version: string; size: string | null };

const BADGE: Record<Phase, string> = {
  connected: 'Tunnel on',
  disconnected: 'Tunnel off',
  connecting: 'Opening',
  disconnecting: 'Closing',
};

/** The app's title-bar badge. Here it also switches the tunnel, from anywhere on the site. */
function TunnelBadge() {
  const phase = useTunnelPhase();
  const on = phase === 'connected';
  return (
    <button
      type="button"
      className="badge"
      data-phase={phase}
      onClick={toggle}
      aria-pressed={on}
      aria-label={`${BADGE[phase]}. Press to turn the demo tunnel ${on ? 'off' : 'on'}`}
    >
      <span className="badge__dot" aria-hidden="true" />
      <span className="badge__text">{BADGE[phase]}</span>
      <span className="badge__short" aria-hidden="true">
        {phase === 'connected' || phase === 'connecting' ? 'On' : 'Off'}
      </span>
    </button>
  );
}

export function SiteHeader({ download }: { download: HeaderDownload }) {
  const pathname = usePathname();
  const [hot, setHot] = useState<string | null>(null);
  // The pill under the hovered link: one element that slides between links (CSS clip-path).
  const [mark, setMark] = useState({ left: 0, width: 0 });
  const [open, setOpen] = useState(false);
  const menuButton = useRef<HTMLButtonElement>(null);

  const current = nav.find((item) => item.href === pathname)?.href ?? null;

  const point = (href: string, link: HTMLElement) => {
    setHot(href);
    setMark({ left: link.offsetLeft, width: link.offsetWidth });
  };

  const close = () => {
    setOpen(false);
    menuButton.current?.focus();
  };

  return (
    <>
      <header className="site-header">
        <div className="site-header__bar">
          <Link href="/" className="brand" aria-label="OpenWire home">
            <Image src="/logo.svg" alt="" width={32} height={32} priority />
            <span aria-hidden="true">OpenWire</span>
          </Link>

          <nav
            className="nav"
            aria-label="Main"
            data-marked={hot ? '' : undefined}
            style={{ '--mark-left': `${mark.left}px`, '--mark-width': `${mark.width}px` } as CSSProperties}
            onPointerLeave={() => setHot(null)}
          >
            <span className="nav__marker" aria-hidden="true" />
            {nav.map((item) => (
              <Link
                key={item.href}
                href={item.href}
                className={`nav__link${hot === item.href ? ' is-hot' : ''}`}
                aria-current={current === item.href ? 'page' : undefined}
                onPointerEnter={(e) => point(item.href, e.currentTarget)}
                onFocus={(e) => point(item.href, e.currentTarget)}
                onBlur={() => setHot(null)}
              >
                <span>{item.label}</span>
              </Link>
            ))}
          </nav>

          <div className="header-tools">
            <TunnelBadge />
            <a
              className="icon-btn"
              href={site.repo}
              target="_blank"
              rel="noopener noreferrer"
              data-fill
              aria-label="OpenWire on GitHub (opens in a new tab)"
            >
              <span className="pill__fill" aria-hidden="true" />
              <Icon icon={GithubIcon} />
            </a>
            <Link className="pill pill--primary pill--sm header-download" href="/download">
              <Roll text="Download" />
            </Link>
            <button
              ref={menuButton}
              type="button"
              className="pill pill--ghost pill--sm menu-btn"
              aria-expanded={open}
              aria-controls="menu"
              onClick={() => setOpen((value) => !value)}
            >
              <Icon icon={open ? Cancel01Icon : Menu01Icon} size={18} />
              <span>{open ? 'Close' : 'Menu'}</span>
            </button>
          </div>
        </div>
      </header>

      <AnimatePresence>
        {open && <MobileMenu key="menu" pathname={pathname} download={download} onClose={close} />}
      </AnimatePresence>
    </>
  );
}

/** The phone menu. While it is open the page behind it is inert, so Tab stays inside the
 *  header and the menu, and Escape closes it. */
function MobileMenu({
  pathname,
  download,
  onClose,
}: {
  pathname: string;
  download: HeaderDownload;
  onClose: () => void;
}) {
  const sheet = useRef<HTMLElement>(null);
  const reduceMotion = useReducedMotion();
  const onEscape = useEffectEvent((e: KeyboardEvent) => {
    if (e.key === 'Escape') onClose();
  });

  useEffect(() => {
    const behind = [document.getElementById('main'), document.querySelector<HTMLElement>('footer.finale')];
    behind.forEach((el) => el && (el.inert = true));
    stopScroll();
    sheet.current?.querySelector<HTMLElement>('a')?.focus();
    document.addEventListener('keydown', onEscape);
    return () => {
      behind.forEach((el) => el && (el.inert = false));
      document.removeEventListener('keydown', onEscape);
      startScroll();
    };
  }, []);

  const links = [...nav, { label: 'Download', href: '/download' }];
  const from = 'circle(0% at calc(100% - 60px) 46px)';
  const to = 'circle(150% at calc(100% - 60px) 46px)';

  return (
    <m.nav
      ref={sheet}
      id="menu"
      className="menu"
      aria-label="Menu"
      initial={reduceMotion ? { opacity: 0 } : { clipPath: from }}
      animate={reduceMotion ? { opacity: 1 } : { clipPath: to }}
      exit={reduceMotion ? { opacity: 0 } : { clipPath: from }}
      transition={{ duration: reduceMotion ? 0.2 : 0.7, ease: EASE }}
    >
      <ul className="menu__list">
        {links.map((item, i) => (
          <m.li
            key={item.href}
            initial={reduceMotion ? false : { y: 36, opacity: 0 }}
            animate={{ y: 0, opacity: 1, transition: { delay: 0.12 + i * 0.05, duration: 0.6, ease: EASE } }}
          >
            <Link
              href={item.href}
              className="menu__link"
              aria-current={item.href === pathname ? 'page' : undefined}
              onClick={onClose}
            >
              {item.label}
              <Icon icon={ArrowRight02Icon} />
            </Link>
          </m.li>
        ))}
      </ul>
      <div className="menu__foot">
        <a className="pill pill--primary pill--lg" href={download.href}>
          <WindowsLogo />
          <span>Download {download.version}</span>
          {download.size && <span className="pill__meta">{download.size}</span>}
        </a>
        <a className="pill pill--ghost pill--lg" href={site.repo} target="_blank" rel="noopener noreferrer">
          <Icon icon={GithubIcon} />
          <span>View on GitHub</span>
          <span className="sr-only">(opens in a new tab)</span>
        </a>
      </div>
    </m.nav>
  );
}
