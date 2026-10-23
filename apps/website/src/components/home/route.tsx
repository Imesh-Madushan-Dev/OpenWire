import Image from 'next/image';
import type { CSSProperties } from 'react';
import { EarthIcon, LaptopIcon, ServerStack01Icon } from '@hugeicons/core-free-icons';

import { how } from '@/data/site';
import { Icon } from '../icon';
import { TunnelSwitch } from '../tunnel-switch';

/*
 * The route your traffic takes, drawn twice: wide (1200x420) and tall (360x820) for phones.
 * Node positions are the same points as the paths, as percentages of the viewBox, so the HTML
 * nodes sit exactly on the lines at any size. src/motion/route.ts runs packets along whichever
 * path is live: through the tunnel while it is on, straight out while it is off.
 */

// Labels sit above the nodes on wide screens, so the direct path can pass underneath them.
const WIDE = {
  line: 'M120 175 L1080 175',
  pipe: 'M390 175 L870 175',
  direct: 'M120 175 C120 320 200 350 330 350 L870 350 C1000 350 1080 320 1080 175',
};
const TALL = {
  line: 'M110 70 L110 750',
  pipe: 'M110 260 L110 580',
  direct: 'M110 70 C250 70 320 100 320 200 L320 620 C320 720 250 750 110 750',
};

const at = (x: number, y: number, xm: number, ym: number) =>
  ({
    '--x': `${(x / 1200) * 100}%`,
    '--y': `${(y / 420) * 100}%`,
    '--xm': `${(xm / 360) * 100}%`,
    '--ym': `${(ym / 820) * 100}%`,
  }) as CSSProperties;

const PACKETS = 6;

function Paths({ d, view }: { d: typeof WIDE; view: string }) {
  return (
    <>
      <path className="route__direct" d={d.direct} data-path="direct" />
      <path className="route__line" d={d.line} data-path="tunnel" />
      <path className="route__pipe" d={d.pipe} />
      <path className="route__core" d={d.pipe} />
      <g data-packets={view}>
        {Array.from({ length: PACKETS }, (_, i) => (
          <circle key={i} className={`packet ${i % 2 ? 'packet--down' : 'packet--up'}`} r="5" cx="-20" cy="-20" />
        ))}
      </g>
    </>
  );
}

export function Route() {
  const { nodes } = how;
  return (
    <section id="how" className="section how" aria-labelledby="how-title">
      <div className="frame">
        <div className="how__head">
          <div className="section-head">
            <h2 id="how-title" className="h-xl" data-split="lines">
              {how.title}
            </h2>
            <p className="lede" data-reveal="up">
              {how.lede}
            </p>
          </div>
          <div className="switch-wrap" data-reveal="up" data-delay="0.1">
            <TunnelSwitch />
            <p>{how.switchCaption}</p>
          </div>
        </div>

        <div className="route" data-route data-reveal="clip">
          <svg className="route__svg route__svg--wide" viewBox="0 0 1200 420" aria-hidden="true">
            <Paths d={WIDE} view="wide" />
          </svg>
          <svg className="route__svg route__svg--tall" viewBox="0 0 360 820" aria-hidden="true">
            <Paths d={TALL} view="tall" />
          </svg>

          <ol className="route__nodes" aria-label="The route your traffic takes">
            <li className="node" style={at(120, 175, 110, 70)}>
              <Icon icon={LaptopIcon} />
              <span className="node__text">
                <span className="node__label">{nodes.apps.label}</span>
                <span className="node__detail">{nodes.apps.detail}</span>
              </span>
            </li>
            <li className="node node--openwire node--tunnelled" style={at(390, 175, 110, 260)}>
              <Image src="/logo.svg" alt="" width={30} height={30} />
              <span className="node__text">
                <span className="node__label">{nodes.openwire.label}</span>
                <span className="node__detail mono">{nodes.openwire.detail}</span>
              </span>
            </li>
            <li className="node node--tunnelled" style={at(870, 175, 110, 580)}>
              <Icon icon={ServerStack01Icon} />
              <span className="node__text">
                <span className="node__label">{nodes.server.label}</span>
                <span className="node__detail">{nodes.server.detail}</span>
              </span>
            </li>
            <li className="node" style={at(1080, 175, 110, 750)}>
              <Icon icon={EarthIcon} />
              <span className="node__text">
                <span className="node__label">{nodes.internet.label}</span>
                <span className="node__detail">{nodes.internet.detail}</span>
              </span>
            </li>
          </ol>

          <p className="route__note route__note--tunnel">{nodes.tunnel}</p>
          <p className="route__note route__note--direct">{nodes.direct}</p>
        </div>

        <ol className="steps">
          {how.steps.map((step, i) => (
            <li key={step.title} data-reveal="up" data-delay={String(i * 0.06)}>
              <span className="steps__n">{String(i + 1).padStart(2, '0')}</span>
              <h3>{step.title}</h3>
              <p>{step.text}</p>
            </li>
          ))}
        </ol>
      </div>
    </section>
  );
}
