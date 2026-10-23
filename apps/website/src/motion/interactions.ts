// Pointer craft shared by every page.
import { gsap } from 'gsap';
import { ease, isTouch, onPage, onScroll, prefersReducedMotion } from '@/engine';

/** The header steps aside while you read down, and comes back as soon as you scroll up. */
export function frameAside() {
  const root = document.documentElement;
  let last = 0;
  onScroll(({ y }) => {
    if (Math.abs(y - last) < 6) return;
    root.classList.toggle('frame-away', y > 180 && y > last);
    last = y;
  });
  onPage(() => {
    root.classList.remove('frame-away');
    last = 0;
  });
}

/** Secondary pills flood with ink from the side the pointer came in, and drain toward where it left. */
export function pointerFill() {
  onPage(() => {
    if (isTouch() || prefersReducedMotion()) return; // CSS covers these
    const off: (() => void)[] = [];
    document.querySelectorAll<HTMLElement>('[data-fill]').forEach((btn) => {
      const fill = btn.querySelector<HTMLElement>('.pill__fill');
      if (!fill) return;
      const from = (e: PointerEvent) => {
        const r = btn.getBoundingClientRect();
        return `${e.clientX - r.left}px ${e.clientY - r.top}px`;
      };
      let closed = true;
      const open = (at: string) => {
        if (closed) gsap.set(fill, { clipPath: `circle(0% at ${at})` });
        closed = false;
        btn.classList.add('is-filled');
        gsap.to(fill, { clipPath: `circle(150% at ${at})`, duration: 0.6, ease: ease.soft, overwrite: true });
      };
      const close = (at: string) => {
        btn.classList.remove('is-filled');
        gsap.to(fill, {
          clipPath: `circle(0% at ${at})`,
          duration: 0.5,
          ease: ease.soft,
          overwrite: true,
          onComplete: () => {
            closed = true;
          },
        });
      };
      const enter = (e: PointerEvent) => open(from(e));
      const leave = (e: PointerEvent) => close(from(e));
      const focus = () => btn.matches(':focus-visible') && open('50% 50%');
      const blur = () => close('50% 50%');
      btn.addEventListener('pointerenter', enter);
      btn.addEventListener('pointerleave', leave);
      btn.addEventListener('focus', focus);
      btn.addEventListener('blur', blur);
      off.push(() => {
        btn.removeEventListener('pointerenter', enter);
        btn.removeEventListener('pointerleave', leave);
        btn.removeEventListener('focus', focus);
        btn.removeEventListener('blur', blur);
        btn.classList.remove('is-filled');
        gsap.set(fill, { clearProps: 'clipPath' });
      });
    });
    return () => off.forEach((f) => f());
  });
}
