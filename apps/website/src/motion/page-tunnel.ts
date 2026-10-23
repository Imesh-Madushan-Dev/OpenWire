// Wires the tunnel to pages: the first page opens it on arrival; the 404 holds it off.
import { onPage } from '@/engine';
import { arrive, pin } from './tunnel';

export function startTunnel() {
  let first = true;
  onPage(() => {
    const off = Boolean(document.querySelector('[data-tunnel-pin="off"]'));
    // Deferred out of the page's gsap.context: the tunnel outlives pages, so its tweens must
    // never be reverted with them.
    queueMicrotask(() => {
      if (first) {
        first = false;
        if (off) pin(true);
        else arrive();
        return;
      }
      pin(off);
    });
  });
}
