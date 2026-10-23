'use client';
// Next.js App Router adapter. Render <Engine setup={...} /> once, as the LAST child of <body> in the
// root layout (React runs child effects first, so page components register before the engine boots).
// `setup` (global motion: transition, driver modules) must come from a client module.
// Internal links (plain <a> or next/link) get the leave -> push -> enter transition automatically.
// Opt a link out with data-no-transition. Back/forward re-inits the page without the curtain.
import { useEffect, useRef } from 'react';
import { usePathname, useRouter } from 'next/navigation';
import { boot, cleanup, enter, leave, onPage, scrollToTarget, type PageFn } from './index';

// The page segment hydrates after the layout that holds <Engine />. Booting before it would let
// SplitText rewrite text React has not hydrated yet: React then discards the page and renders it
// again, unrevealed. Each page renders <PageReady /> last, and the engine waits for it.
let markReady = () => {};
const pageHydrated = new Promise<void>((resolve) => {
  markReady = resolve;
});

/** Render as the last element of every page: tells the engine the page has hydrated. */
export function PageReady() {
  useEffect(() => markReady(), []);
  return null;
}

/** How long to wait for a page without <PageReady /> before booting anyway. */
const READY_FALLBACK_MS = 4000;
/** The most the leave animation may hold a navigation back. */
const LEAVE_CAP_MS = 1500;
/** How long a pushed route may take to commit before the curtain lifts on the current page. */
const COMMIT_CAP_MS = 8000;

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export function Engine({ setup }: { setup?: () => void }) {
  const router = useRouter();
  const pathname = usePathname();
  const mode = useRef<'idle' | 'push' | 'pop'>('idle');
  const first = useRef(true);
  const shown = useRef(pathname);
  const watchdog = useRef<number | undefined>(undefined);

  useEffect(() => {
    let started = false;
    const start = () => {
      if (started) return;
      started = true;
      boot(setup);
    };
    pageHydrated.then(start);
    const fallback = setTimeout(start, READY_FALLBACK_MS);
    return () => clearTimeout(fallback);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const onClick = async (e: MouseEvent) => {
      if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
      const a = (e.target as Element).closest('a');
      if (!a || (a.target && a.target !== '_self') || a.hasAttribute('download') || a.hasAttribute('data-no-transition')) return;
      const url = new URL(a.href, location.href);
      if (url.origin !== location.origin) return; // external
      if (url.pathname === location.pathname) {
        // Same page: glide to a #section instead of jumping; leave plain same-page links alone.
        const target = url.hash && document.getElementById(decodeURIComponent(url.hash.slice(1)));
        if (!target) return;
        e.preventDefault();
        history.replaceState(history.state, '', url.hash);
        scrollToTarget(target);
        // Move focus too, as a native jump would: the skip link and keyboard users depend on it.
        if (!target.hasAttribute('tabindex')) target.setAttribute('tabindex', '-1');
        target.focus({ preventScroll: true });
        return;
      }
      e.preventDefault(); // next/link sees defaultPrevented and stands down
      if (mode.current !== 'idle') return;
      mode.current = 'push';
      const href = url.pathname + url.search + url.hash;
      router.prefetch(href);
      // Never wait on the curtain for long: a navigation must always happen.
      await Promise.race([leave(), wait(LEAVE_CAP_MS)]);
      cleanup();
      router.push(href, { scroll: false });
      // If the route never commits (a failed fetch, an interrupted push), lift the curtain on the
      // page we are still on rather than leave the visitor in the dark.
      clearTimeout(watchdog.current);
      watchdog.current = window.setTimeout(() => {
        if (mode.current !== 'push') return;
        console.warn(`[engine] navigation to ${href} did not commit; restoring the page`);
        mode.current = 'idle';
        enter({ transition: true, scrollTop: false });
      }, COMMIT_CAP_MS);
    };
    const onPop = () => {
      // A history step that only changes the #hash stays on this page: nothing to tear down.
      if (location.pathname === shown.current) return;
      mode.current = 'pop';
      cleanup();
    };
    document.addEventListener('click', onClick, true);
    addEventListener('popstate', onPop);
    return () => {
      document.removeEventListener('click', onClick, true);
      removeEventListener('popstate', onPop);
    };
  }, [router]);

  useEffect(() => {
    shown.current = pathname;
    clearTimeout(watchdog.current);
    if (first.current) {
      first.current = false;
      return;
    }
    const pop = mode.current === 'pop';
    mode.current = 'idle';
    enter({ transition: !pop, scrollTop: !pop });
  }, [pathname]);

  return null;
}

/** Component-level page code: runs when the page is ready, reverted on leave. */
export function usePage(fn: PageFn) {
  useEffect(
    () => onPage(fn),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );
}
