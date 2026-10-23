'use client';

import { toggle } from '@/motion/tunnel';
import { useTunnelPhase } from './power-orb';

const LABEL = {
  connected: 'Tunnel on',
  disconnected: 'Tunnel off',
  connecting: 'Opening the tunnel',
  disconnecting: 'Closing the tunnel',
} as const;

/** A second handle on the same demo tunnel, beside the diagram it changes. */
export function TunnelSwitch() {
  const phase = useTunnelPhase();
  return (
    <button type="button" role="switch" className="switch" aria-checked={phase === 'connected'} onClick={toggle}>
      <span className="switch__track" aria-hidden="true">
        <span className="switch__thumb" />
      </span>
      <span>{LABEL[phase]}</span>
    </button>
  );
}
