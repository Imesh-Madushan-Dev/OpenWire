// Global motion for the site: runs once on the client, before the first page initializes.
import { startTunnel } from './page-tunnel';
import { ringTransition } from './transition';
import { frameAside, pointerFill } from './interactions';
import { orbMorph } from './orb-morph';
import { chapters, finaleWire, pasteDemo, routePackets, statementWords } from './sections';

export function setupMotion() {
  ringTransition();
  startTunnel();
  frameAside();
  pointerFill();
  orbMorph();
  statementWords();
  chapters();
  routePackets();
  pasteDemo();
  finaleWire();
}
