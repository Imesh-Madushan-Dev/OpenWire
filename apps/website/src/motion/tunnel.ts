// The concept's driver: one tunnel, shared by every part of the page that shows it.
//
// `phase` is the app's own state machine (status-headline.tsx), for the words and the orb.
// `level` is the light: 0 is off, 1 is on. It is tweened on the GSAP ticker and written to
// `--tunnel` on <html>, where `--state` mixes the on and off colours from it, so the room
// light, the badge, the button glow and the route diagram all change together.
//
// No DOM access at import time: this module also loads on the server.
import { gsap } from 'gsap';
import { prefersReducedMotion } from '@/engine/env';

export type Phase = 'disconnected' | 'connecting' | 'connected' | 'disconnecting';

/** How long the demo handshake takes. The real one takes as long as the server does. */
export const CONNECT_S = 1.2;
const DISCONNECT_S = 0.7;
/** The beat of darkness on arrival before the tunnel starts to open. */
const ARRIVE_DELAY_S = 0.3;

let phase: Phase = 'connected';
const light = { level: 1 };
const listeners = new Set<() => void>();
let pending: gsap.core.Tween | null = null;
let pinned = false;

export const subscribe = (fn: () => void) => {
  listeners.add(fn);
  return () => {
    listeners.delete(fn);
  };
};
export const getPhase = () => phase;
/** The server renders the tunnel on: it is the frame visitors without JavaScript see. */
export const getServerPhase = (): Phase => 'connected';
export const tunnelLevel = () => light.level;
export const isBusy = (p: Phase) => p === 'connecting' || p === 'disconnecting';

function set(next: Phase) {
  phase = next;
  document.documentElement.dataset.tunnel = next;
  listeners.forEach((fn) => fn());
}

function shine(level: number, duration: number, ease = 'power2.inOut') {
  const write = () => document.documentElement.style.setProperty('--tunnel', light.level.toFixed(3));
  if (prefersReducedMotion() || duration === 0) {
    gsap.killTweensOf(light);
    light.level = level;
    write();
    return;
  }
  gsap.to(light, { level, duration, ease, overwrite: true, onUpdate: write });
}

function schedule(seconds: number, fn: () => void) {
  pending?.kill();
  pending = gsap.delayedCall(prefersReducedMotion() ? 0.25 : seconds, fn);
}

export function connect() {
  if (phase === 'connected' || phase === 'connecting') return;
  set('connecting');
  // The light warms as the count climbs, and only floods once the tunnel is up.
  shine(0.35, CONNECT_S, 'power1.in');
  schedule(CONNECT_S, () => {
    set('connected');
    shine(1, 0.8, 'expo.out');
  });
}

export function disconnect() {
  if (phase === 'disconnected' || phase === 'disconnecting') return;
  set('disconnecting');
  shine(0, DISCONNECT_S);
  schedule(DISCONNECT_S, () => set('disconnected'));
}

/** The orb, the badge and the route switch all call this. Ignored mid-change, like the app. */
export function toggle() {
  if (isBusy(phase) || pinned) return;
  if (phase === 'connected') disconnect();
  else connect();
}

/** First load: the room starts dark, then the tunnel opens on its own. */
export function arrive() {
  if (prefersReducedMotion()) {
    set('connected');
    shine(1, 0);
    return;
  }
  pending?.kill();
  set('disconnected');
  shine(0, 0);
  schedule(ARRIVE_DELAY_S, connect);
}

/** A page can hold the tunnel off (the 404). Leaving it opens the tunnel again. */
export function pin(off: boolean) {
  if (off && !pinned) {
    pinned = true;
    pending?.kill();
    set('disconnected');
    shine(0, 0.6);
  } else if (!off && pinned) {
    pinned = false;
    schedule(0.3, connect);
  }
}
