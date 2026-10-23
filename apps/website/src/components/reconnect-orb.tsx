'use client';

import { PowerOrb } from './power-orb';

/**
 * The 404's orb: held off, and pressing it reconnects by taking you home. It clicks the page's
 * own "Back to the start" link, so the trip goes through the site's usual transition.
 */
export function ReconnectOrb() {
  const reconnect = () => document.querySelector<HTMLAnchorElement>('[data-reconnect]')?.click();
  return <PowerOrb pinned="disconnected" onPress={reconnect} pressLabel="Reconnect: back to the start" />;
}
