/**
 * Captures the README screenshots from the real UI running on made-up data
 * (screenshots.html -> src/demo/main.tsx), so no real address, device or
 * server ever ends up in the repository.
 *
 * Starts Vite, drives a headless Chrome or Edge over the DevTools protocol at
 * the app's own window size, and writes compressed WebP images, with
 * transparent corners, to docs/screenshots at the repository root. Needs
 * Node (for Vite) and Chrome or Edge. From apps/desktop-app:
 *
 *   bun scripts/screenshots.ts
 */

import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'

const PORT = 5288
const DEBUG_PORT = 9333
const WIDTH = 820
const HEIGHT = 520
const SCALE = 2
/** WebP quality: sharp UI text at roughly a fifth of the PNG's size. */
const QUALITY = 82
/** Long enough for data to load and every entrance animation to finish. */
const SETTLE_MS = 3500

/** Every page light and dark - Aurora light, Graphite dark - for the README's pairs. */
const PAGES = [
  { file: 'vpn', view: 'vpn' },
  { file: 'network', view: 'network' },
  { file: 'usage', view: 'usage' },
  { file: 'devices', view: 'devices' },
  { file: 'settings', view: 'settings', section: 'appearance' },
]
const SHOTS = PAGES.flatMap(({ file, ...page }) => [
  { ...page, file, theme: 'aurora', mode: 'light' },
  { ...page, file: `${file}-dark`, theme: 'graphite', mode: 'dark' },
])

const appDir = join(import.meta.dir, '..')
const outDir = join(appDir, '..', '..', 'docs', 'screenshots')

const BROWSERS = [
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  '/usr/bin/google-chrome',
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
]

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

async function waitFor(url: string) {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(url)).ok) return
    } catch {
      // Not listening yet.
    }
    await sleep(200)
  }
  throw new Error(`nothing answered at ${url}`)
}

/** A minimal DevTools protocol client: one page, request/response only. */
async function connect(wsUrl: string) {
  const socket = new WebSocket(wsUrl)
  await new Promise((resolve, reject) => {
    socket.onopen = resolve
    socket.onerror = reject
  })
  let id = 0
  const pending = new Map<number, (result: Record<string, unknown>) => void>()
  socket.onmessage = (event) => {
    const message = JSON.parse(String(event.data))
    pending.get(message.id)?.(message.result ?? {})
    pending.delete(message.id)
  }
  return {
    send(method: string, params: Record<string, unknown> = {}) {
      return new Promise<Record<string, unknown>>((resolve) => {
        pending.set(++id, resolve)
        socket.send(JSON.stringify({ id, method, params }))
      })
    },
    close: () => socket.close(),
  }
}

const browser = BROWSERS.find((path) => existsSync(path))
if (!browser) throw new Error('No Chrome or Edge found - add its path to BROWSERS.')

mkdirSync(outDir, { recursive: true })
const profile = mkdtempSync(join(tmpdir(), 'openwire-shots-'))

// Vite's own entry under Node, one process: `bunx` would leave a child
// running when killed.
const viteBin = join(dirname(createRequire(join(appDir, 'package.json')).resolve('vite/package.json')), 'bin', 'vite.js')
const vite = Bun.spawn(['node', viteBin, '--port', String(PORT), '--strictPort'], {
  cwd: appDir,
  stdout: 'ignore',
  stderr: 'inherit',
})
const chrome = Bun.spawn(
  [
    browser,
    '--headless=new',
    `--remote-debugging-port=${DEBUG_PORT}`,
    `--user-data-dir=${profile}`,
    '--disable-extensions',
    '--hide-scrollbars',
    '--no-first-run',
    'about:blank',
  ],
  { stdout: 'ignore', stderr: 'ignore' },
)

try {
  await waitFor(`http://localhost:${PORT}/screenshots.html`)
  await waitFor(`http://localhost:${DEBUG_PORT}/json/version`)

  const targets = (await (await fetch(`http://localhost:${DEBUG_PORT}/json`)).json()) as {
    type: string
    webSocketDebuggerUrl: string
  }[]
  const page = await connect(targets.find((target) => target.type === 'page')!.webSocketDebuggerUrl)

  await page.send('Emulation.setDeviceMetricsOverride', {
    width: WIDTH,
    height: HEIGHT,
    deviceScaleFactor: SCALE,
    mobile: false,
  })
  // The window's corners are rounded; outside them stays see-through.
  await page.send('Emulation.setDefaultBackgroundColorOverride', {
    color: { r: 0, g: 0, b: 0, a: 0 },
  })

  for (const { file, ...params } of SHOTS) {
    const query = new URLSearchParams(params)
    await page.send('Page.navigate', { url: `http://localhost:${PORT}/screenshots.html?${query}` })
    await sleep(SETTLE_MS)
    const { data } = await page.send('Page.captureScreenshot', { format: 'webp', quality: QUALITY })
    const bytes = Buffer.from(String(data), 'base64')
    writeFileSync(join(outDir, `${file}.webp`), bytes)
    console.log(`docs/screenshots/${file}.webp  ${Math.round(bytes.length / 1024)} KB`)
  }
  page.close()
} finally {
  chrome.kill()
  vite.kill()
  await Promise.all([chrome.exited, vite.exited])
  rmSync(profile, { recursive: true, force: true })
}
process.exit(0)
