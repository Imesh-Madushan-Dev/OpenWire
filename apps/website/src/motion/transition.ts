// The page transition: a tunnel ring. Leaving, a ring closes the page from the point you
// clicked, carrying the next page's name. Arriving, it opens again from the middle onto the
// new page. The ring's colour is the tunnel's (--state), so it is cyan or pink to match.
import { gsap } from 'gsap';
import { ease, setTransition } from '@/engine';

const NAMES: Record<string, string> = {
  '/': 'OpenWire',
  '/download': 'Download',
  '/privacy': 'Privacy',
};

let point = { x: 0, y: 0 };
let label = '';

const reach = (x: number, y: number) => Math.hypot(Math.max(x, innerWidth - x), Math.max(y, innerHeight - y)) + 4;

export function ringTransition() {
  point = { x: innerWidth / 2, y: innerHeight / 2 };
  addEventListener('pointerdown', (e) => (point = { x: e.clientX, y: e.clientY }), { capture: true, passive: true });
  // Registered before the router adapter's listener, so the name is ready when leave() runs.
  document.addEventListener(
    'click',
    (e) => {
      const a = (e.target as Element).closest?.('a');
      if (!a) return;
      const url = new URL(a.href, location.href);
      const section = url.hash ? a.textContent?.trim() : '';
      label = section || NAMES[url.pathname] || a.textContent?.trim() || '';
      // Keyboard activation has no pointer: open from the link itself.
      if (e.detail === 0) {
        const r = a.getBoundingClientRect();
        point = { x: r.left + r.width / 2, y: r.top + r.height / 2 };
      }
    },
    true,
  );

  setTransition({
    leave: (c) => {
      const panel = c.querySelector<HTMLElement>('.curtain__panel')!;
      const ring = c.querySelector('circle')!;
      const text = c.querySelector<HTMLElement>('[data-curtain-label]')!;
      const { x, y } = point;
      const r = reach(x, y);
      text.textContent = label;
      return gsap
        .timeline()
        .set(c, { autoAlpha: 1 })
        .set(panel, { maskImage: 'none', webkitMaskImage: 'none' })
        .set(ring, { attr: { cx: x, cy: y, r: 0 }, opacity: 1 })
        .set(text, { yPercent: 115 })
        .fromTo(panel, { clipPath: `circle(0px at ${x}px ${y}px)` }, { clipPath: `circle(${r}px at ${x}px ${y}px)`, duration: 0.75, ease: ease.inOut }, 0)
        .to(ring, { attr: { r }, duration: 0.75, ease: ease.inOut }, 0)
        .to(ring, { opacity: 0, duration: 0.25 }, 0.5)
        .to(text, { yPercent: 0, duration: 0.6, ease: ease.out }, 0.35);
    },
    enter: (c) => {
      const panel = c.querySelector<HTMLElement>('.curtain__panel')!;
      const ring = c.querySelector('circle')!;
      const text = c.querySelector<HTMLElement>('[data-curtain-label]')!;
      const x = innerWidth / 2;
      const y = innerHeight / 2;
      const r = reach(x, y);
      const hole = { r: 0 };
      const cut = () => {
        const mask = `radial-gradient(circle at ${x}px ${y}px, transparent ${hole.r}px, #000 ${hole.r + 1}px)`;
        panel.style.maskImage = mask;
        panel.style.webkitMaskImage = mask;
      };
      return gsap
        .timeline()
        .set(panel, { clipPath: 'none' })
        .to(text, { yPercent: -115, duration: 0.35, ease: 'power3.in' })
        .set(ring, { attr: { cx: x, cy: y, r: 0 }, opacity: 1 }, '<0.12')
        .add(cut)
        .to(hole, { r, duration: 0.85, ease: ease.inOut, onUpdate: cut }, '<')
        .to(ring, { attr: { r }, duration: 0.85, ease: ease.inOut }, '<')
        .to(ring, { opacity: 0, duration: 0.3 }, '>-0.3')
        .set(c, { autoAlpha: 0 })
        .set(panel, { maskImage: 'none', webkitMaskImage: 'none' });
    },
  });
}
