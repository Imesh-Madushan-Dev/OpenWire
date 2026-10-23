// Scroll choreography for the home page sections and the footer.
import { gsap } from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import { SplitText } from 'gsap/SplitText';
import { ease, onPage, prefersReducedMotion } from '@/engine';
import { tunnelLevel } from './tunnel';

gsap.registerPlugin(ScrollTrigger, SplitText);

const reduced = () => prefersReducedMotion();

/** The statement's words go from dim to ink as you read down: a signal passing along the line. */
export function statementWords() {
  onPage(() => {
    const el = document.querySelector<HTMLElement>('[data-statement]');
    if (!el || reduced()) return;
    const split = SplitText.create(el, { type: 'words', wordsClass: 'word' });
    gsap.fromTo(
      split.words,
      { opacity: 0.16 },
      {
        opacity: 1,
        stagger: 0.12,
        ease: 'none',
        scrollTrigger: { trigger: el, start: 'top 82%', end: 'bottom 48%', scrub: true },
      },
    );
    return () => split.revert();
  });
}

/** Features: the sticky window swaps to the chapter in view, and the tab marker follows. */
export function chapters() {
  onPage(() => {
    const root = document.querySelector<HTMLElement>('[data-chapters]');
    if (!root) return;
    const mm = gsap.matchMedia();
    mm.add('(min-width: 1024px)', () => {
      const items = gsap.utils.toArray<HTMLElement>('[data-chapter]', root);
      const shots = gsap.utils.toArray<HTMLElement>('[data-shot]', root);
      const tabs = gsap.utils.toArray<HTMLElement>('[data-tab]', root);
      const nav = root.querySelector<HTMLElement>('[data-tabs]')!;
      const marker = root.querySelector<HTMLElement>('[data-tabs-marker]')!;
      let active = 0;

      const place = (i: number, animate = true) => {
        const n = nav.getBoundingClientRect();
        const t = tabs[i].getBoundingClientRect();
        const left = t.left - n.left - 5;
        const right = n.right - 5 - t.right;
        gsap.to(marker, {
          clipPath: `inset(0px ${right}px 0px ${left}px round 999px)`,
          duration: animate && !reduced() ? 0.7 : 0,
          ease: ease.out,
          overwrite: true,
        });
      };

      const show = (i: number) => {
        if (i === active) return;
        const prev = shots[active];
        const next = shots[i];
        const dir = i > active ? 1 : -1;
        items.forEach((item, k) => item.classList.toggle('is-active', k === i));
        tabs.forEach((tab, k) => (k === i ? tab.setAttribute('aria-current', 'true') : tab.removeAttribute('aria-current')));
        shots.forEach((shot) => {
          if (shot !== prev && shot !== next) gsap.set(shot, { opacity: 0, zIndex: 0 });
        });
        gsap.set(prev, { zIndex: 1 });
        gsap.set(next, { zIndex: 2, opacity: 1 });
        gsap.fromTo(
          next,
          { clipPath: dir > 0 ? 'inset(0% 0% 0% 100%)' : 'inset(0% 100% 0% 0%)', scale: 1.04 },
          {
            clipPath: 'inset(0% 0% 0% 0%)',
            scale: 1,
            duration: reduced() ? 0 : 1,
            ease: ease.out,
            overwrite: true,
            onComplete: () => {
              if (shots[active] !== prev) gsap.set(prev, { opacity: 0 });
            },
          },
        );
        place(i);
        active = i;
      };

      gsap.set(shots, { opacity: (k: number) => (k === 0 ? 1 : 0) });
      items.forEach((item, i) =>
        ScrollTrigger.create({
          trigger: item,
          start: 'top 55%',
          end: 'bottom 55%',
          onToggle: (self) => self.isActive && show(i),
        }),
      );
      const ro = new ResizeObserver(() => place(active, false));
      ro.observe(nav);
      document.fonts?.ready.then(() => place(active, false));
      return () => ro.disconnect();
    });
    return () => mm.revert();
  });
}

/**
 * How it works: packets run the route on the one clock. Magenta goes out, gold comes back, as in
 * the app's graph. While the tunnel is on they pass through OpenWire and your server; while it is
 * off they take the direct path. They fade out as the light crosses over, and pause off screen.
 */
export function routePackets() {
  onPage(() => {
    const root = document.querySelector<HTMLElement>('[data-route]');
    if (!root || reduced()) return;
    const views = gsap.utils.toArray<SVGSVGElement>('svg.route__svg', root).map((svg) => ({
      svg,
      tunnel: svg.querySelector<SVGPathElement>('[data-path="tunnel"]')!,
      direct: svg.querySelector<SVGPathElement>('[data-path="direct"]')!,
      packets: Array.from(svg.querySelectorAll<SVGCircleElement>('circle.packet')),
    }));
    // Only the drawing on screen has geometry: a display:none SVG measures its paths as 0 long.
    const visible = () => views.find((v) => v.svg.getBoundingClientRect().width > 0) ?? null;
    let view = visible();
    let lengths = { tunnel: 0, direct: 0 };
    const measure = () => {
      view = visible();
      lengths = view ? { tunnel: view.tunnel.getTotalLength(), direct: view.direct.getTotalLength() } : { tunnel: 0, direct: 0 };
    };
    measure();
    const SPEED = 150; // viewBox units per second

    const draw = (time: number) => {
      if (!view) return;
      const level = tunnelLevel();
      const through = level >= 0.5;
      const path = through ? view.tunnel : view.direct;
      const length = through ? lengths.tunnel : lengths.direct;
      if (!(length > 0) || !Number.isFinite(time)) return;
      const fade = Math.min(1, Math.abs(level - 0.5) * 2.4);
      const count = view.packets.length;
      view.packets.forEach((packet, i) => {
        const outgoing = i % 2 === 0;
        let u = (((time * SPEED) / length + i / count) % 1 + 1) % 1;
        if (!outgoing) u = 1 - u;
        const at = u * length;
        if (!Number.isFinite(at)) return;
        const point = path.getPointAtLength(at);
        packet.setAttribute('cx', point.x.toFixed(1));
        packet.setAttribute('cy', point.y.toFixed(1));
        packet.style.opacity = String(Math.min(1, Math.min(u, 1 - u) * 14) * (Number.isFinite(fade) ? fade : 1));
      });
    };
    // A throw inside a ticker callback would stop GSAP's clock, and with it Lenis and every
    // transition on the site. Packets are decoration: on any error they stop, nothing else does.
    let broken = false;
    const tick = (time: number) => {
      try {
        draw(time);
      } catch (error) {
        broken = true;
        gsap.ticker.remove(tick);
        console.warn('[route] packets stopped:', error);
      }
    };

    const onResize = () => measure();
    addEventListener('resize', onResize);
    ScrollTrigger.create({
      trigger: root,
      start: 'top bottom',
      end: 'bottom top',
      onToggle: (self) => (self.isActive && !broken ? gsap.ticker.add(tick) : gsap.ticker.remove(tick)),
    });
    return () => {
      gsap.ticker.remove(tick);
      removeEventListener('resize', onResize);
    };
  });
}

/** Bring your own server: Ctrl and V go down, the link lands in the field, the server appears. */
export function pasteDemo() {
  onPage(() => {
    const root = document.querySelector<HTMLElement>('[data-paste-demo]');
    if (!root || reduced()) return;
    const keys = root.querySelectorAll<HTMLElement>('[data-key]');
    const text = root.querySelector<HTMLElement>('[data-paste-text]')!;
    const added = root.querySelector<HTMLElement>('[data-added]')!;
    const final = text.textContent ?? '';
    const shown = { n: 0 };
    gsap.set(added, { autoAlpha: 0, y: 18 });
    gsap.set(text, { opacity: 0.25 });
    gsap
      .timeline({ scrollTrigger: { trigger: root, start: 'top 72%', once: true }, delay: 0.2 })
      .to(keys[0], { y: 3, scale: 0.94, duration: 0.18, ease: ease.soft })
      .to(keys[1], { y: 3, scale: 0.94, duration: 0.18, ease: ease.soft }, '+=0.12')
      .to(keys, { y: 0, scale: 1, duration: 0.4, ease: ease.spring }, '+=0.18')
      .set(text, { opacity: 1 }, '<')
      // A plain to(): a fromTo would render its empty start the moment it is created.
      .to(
        shown,
        {
          n: final.length,
          duration: 0.7,
          ease: 'power2.out',
          onStart: () => {
            shown.n = 0;
            text.textContent = '';
          },
          onUpdate: () => {
            text.textContent = final.slice(0, Math.round(shown.n));
          },
          onComplete: () => {
            text.textContent = final;
          },
        },
        '<',
      )
      .to(added, { autoAlpha: 1, y: 0, duration: 0.9, ease: ease.spring }, '-=0.1');
    return () => {
      text.textContent = final;
    };
  });
}

/** The logo's wire draws under the wordmark as the page reaches its end. */
export function finaleWire() {
  onPage(() => {
    const wire = document.querySelector<HTMLElement>('[data-wire]');
    if (!wire || reduced()) return;
    gsap.fromTo(
      wire,
      { scaleX: 0 },
      {
        scaleX: 1,
        ease: 'none',
        scrollTrigger: { trigger: wire.parentElement, start: 'top bottom', end: 'bottom bottom', scrub: 0.6 },
      },
    );
  });
}
