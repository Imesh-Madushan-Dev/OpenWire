import type { Metadata } from 'next';
import Link from 'next/link';
import { ArrowRight02Icon } from '@hugeicons/core-free-icons';

import { Icon } from '@/components/icon';
import { Roll } from '@/components/pills';
import { ReconnectOrb } from '@/components/reconnect-orb';
import { PageReady } from '@/engine/next';
import { delay } from '@/lib/enter';

export const metadata: Metadata = {
  title: 'Nothing at the other end',
  description: 'This address is not on the server. Head back to OpenWire’s home page or straight to the download.',
};

/** The tunnel is held off here (src/motion/page-tunnel.ts): there is nothing to connect to.
 *  Pressing the orb reconnects, which here means going back to the start. */
export default function NotFound() {
  return (
    <section className="notfound" data-tunnel-pin="off" aria-labelledby="notfound-title">
      <div className="notfound__inner">
        <div className="orb-slot" data-enter style={delay(0)}>
          <ReconnectOrb />
        </div>
        <p className="label mono" data-enter style={delay(0.12)}>
          404, tunnel is off
        </p>
        <h1 id="notfound-title" className="page-hero__title">
          <span className="rise" style={delay(0.16)}>
            <span>Nothing at the other end.</span>
          </span>
        </h1>
        <p className="lede" style={{ maxWidth: '36ch', ...delay(0.32) }} data-enter="slide">
          This address is not on the server. The tunnel is fine; the page is not here.
        </p>
        <div className="actions" style={{ justifyContent: 'center', ...delay(0.42) }} data-enter>
          <Link className="pill pill--primary pill--lg" href="/" data-reconnect>
            <Roll text="Back to the start" />
            <Icon icon={ArrowRight02Icon} />
          </Link>
          <Link className="pill pill--ghost pill--lg" href="/download" data-fill>
            <span className="pill__fill" aria-hidden="true" />
            <span>Download</span>
          </Link>
        </div>
      </div>
      <PageReady />
    </section>
  );
}
