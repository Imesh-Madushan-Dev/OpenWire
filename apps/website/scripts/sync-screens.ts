/**
 * Copies the README's dark screenshots (docs/screenshots/*-dark.webp, the real
 * UI in the Graphite theme on made-up data) into public/screens, where the
 * site serves them. docs/ stays the one source: regenerate the screenshots
 * there with `bun scripts/screenshots.ts` in apps/desktop-app, then run this.
 * From apps/website:
 *
 *   bun run screens
 */

import { copyFileSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'

const PAGES = ['vpn', 'network', 'usage', 'devices', 'settings']

const siteDir = join(import.meta.dir, '..')
const docsDir = join(siteDir, '..', '..', 'docs', 'screenshots')
const outDir = join(siteDir, 'public', 'screens')

mkdirSync(outDir, { recursive: true })
for (const page of PAGES) {
  copyFileSync(join(docsDir, `${page}-dark.webp`), join(outDir, `${page}.webp`))
  console.log(`public/screens/${page}.webp  <-  docs/screenshots/${page}-dark.webp`)
}
