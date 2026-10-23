// Global motion for the site: runs once on the client, before the first page initializes.
import { startTunnel } from './page-tunnel';
import { ringTransition } from './transition';
import { frameAside, pointerFill } from './interactions';
import { chapters, finaleWire, heroScreens, pasteDemo, routePackets, statementWords } from './sections';

export function setupMotion() {
  ringTransition();
  startTunnel();
  frameAside();
  pointerFill();
  heroScreens();
  statementWords();
  chapters();
  routePackets();
  pasteDemo();
  finaleWire();
}
