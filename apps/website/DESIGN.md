# OpenWire website: design plan

The product site for OpenWire, in `apps/website` (Next.js App Router, Tailwind v4, Bun + Turbo
monorepo). Written before the build so every later pass stays consistent.

## Brief

- **Subject:** OpenWire, a free, open-source (GPL-3.0) network monitor and tunnel for Windows 10
  and 11. Rust core on Tauri 2 with a native WebView2 window, React 19 UI. By Imesh Madushan.
- **Audience:** Windows users who run or share their own VLESS or Trojan servers (v2rayN and
  NetMod users are named in the 0.2.0 notes), GlassWire users who want something free and open,
  and developers who will read the source.
- **Job:** download the installer in one click, or go read the repository.
- **Assets:** the README's dark screenshots in `docs/screenshots` (the real UI in the Graphite
  theme, on the demo's made-up data, copied in with `bun run screens`), the logo from `apps/desktop-app/public`, and live release data from
  GitHub's API (version, date, file names, sizes, notes).
- **Owner's constraints:** dark mode only. Figtree and Hugeicons, as in the app. The desktop
  app's colours. Never bold: semibold (600) is the heaviest weight anywhere. Fully rounded
  corners, breathable buttons and layout. GSAP, Lenis and Motion. A GitHub icon, and a hero
  with a direct download of the latest version plus a "view repo" button. The feel of a
  Silicon Valley product launch.
- **Copy rule (from the app):** connection state is told in tunnel words, "Tunnel is on",
  "Tunnel is off", "Opening the tunnel". Never protected, unprotected or safe: OpenWire disguises
  traffic, it does not promise safety.

## Concepts

| | A. Tunnel on (the product's own screen) | B. This page, live (the traffic graph) | C. The wire (the logo) |
|---|---|---|---|
| Idea | The site is OpenWire's connect screen: it opens a tunnel as you arrive, and every light on the page follows whether the tunnel is on | The hero is a GlassWire-style graph drawing this page's own network traffic, live, in the app's gold and magenta | The wire that leaves the open "O" of the logo runs through the whole page as one scroll-drawn line |
| Anchor | The app's VPN pane: one status line, one power orb, an ambient field that is warm when off and cool when on, and a TUNNEL ON badge in the title bar | The Network tab: download in gold, upload in magenta, updated every second | `generate-logo.ts`: an open O with a gold-to-pink wire leaving through the gap |
| Layout | Split hero: type left, the orb right; the real window below | Full-bleed chart under the hero type | A line threading every section |
| Motion verb | connect: travel along a line, then lock into place | spike and settle | draw |
| Signature | The orb counts up, locks, and recolours the whole page; a click turns it off; scrolling carries it into the app window, onto the app's own orb | Spikes appear as images lazy-load | The line draws as you scroll |
| Screenshot | The conic ring with "Tunnel is on", the app window rising beneath | A gold mountain under the headline | The line around the download button |
| O / F / Fe / P | 4 / 5 / 4 / 3 | 4 / 5 / 3 / 2 | 3 / 4 / 4 / 3 |

**Pick A.** It is the app's real interaction, it gives one variable that can drive the whole page,
and it survives to the footer and the 404 (a tunnel that is off). B fails on its own honesty: a
cached revisit draws a flat line, and upload cannot be measured from a page, so the magenta
series would have to be invented. **Stolen from B** for a detail: the footer counts this visit's
real transfer with the Resource Timing API, "the way OpenWire counts every app". **Stolen from
C:** the logo's gold-to-pink wire is the line that draws under the footer wordmark.

## Concept card

```
Idea:        The website is OpenWire's connect screen: it opens a tunnel when you arrive, and
             every light on the page follows whether the tunnel is on.
Metaphor:    The app's VPN pane: one status line, one power orb, and an ambient light that
             turns warm when the tunnel is off and cool when it is on.
Signature:   The hero power orb, ported from the app's own PowerOrb. It counts "Opening the
             tunnel", locks to "Tunnel is on", and a click turns it off and on again. One
             variable, --tunnel on <html>, recolours the room light, the header badge, the
             button glow and the route diagram. Scrolling carries the orb into the real app
             window, where it lands on the app's own orb. (src/motion/tunnel.ts, orb-morph.ts)
Emotion:     Switching something on: a short count, a click, then the room lights up.
Motion verb: Connect. Things travel along a line, then lock into place with a short settle.
Screenshot:  The hero at rest: a semibold headline on the left, the conic green-cyan-blue ring
             with "Tunnel is on" on the right, the app window rising underneath.
```

Honesty: the orb is labelled as the app's button working on its own; it changes nothing on the
visitor's PC. The window images are the real UI on the demo's made-up data.

## Art direction

### Palette (Aurora dark, chosen by the owner, plus the Classic chart pair)

The site wears Aurora; the product shots show Graphite, the theme the README's dark
screenshots use. Both are the app's own, so the windows read as the same product on an
indigo desk.

| Role | Value | Source |
|---|---|---|
| ground | `#12141d` | Aurora dark `--background` |
| ground-2 | `#1a1d2a` | Aurora dark `--card` |
| ink | `#eceef6` | Aurora dark `--foreground` |
| muted | `#9196ab` (6.3:1 on ground) | Aurora dark `--muted-foreground` |
| line | `rgb(255 255 255 / 0.09)` | Aurora dark `--border` |
| on | `#5eead4` | Aurora dark `--protected`: the tunnel is on |
| off | `#ff8f80` | Aurora dark `--exposed`: the tunnel is off |
| action | `135deg, #5466f5 to #7c4ddb`, white text | Aurora dark `--action`, `--primary-foreground` |
| focus | `#818cf8` | Aurora dark `--ring` |
| ring | conic from 200deg `#34d399, #22d3ee, #60a5fa` | Aurora dark `--protected-ring` |
| room | indigo from the top left, violet from the bottom right | Aurora dark `--canvas` |
| down / up | `#f3c900` / `#d7397f` | Classic chart palette, the logo's wire |
| tones | sky `#38bdf8`, amber `#fbbf24`, violet `#a78bfa`, green `#34d399`, rose `#fb7185` | `--tone-*`, one per app tab |

`--state = color-mix(on, off, --tunnel)` is the only light that changes, and only through
`--tunnel`. Ink never changes, so text reads the same in both states. Every hue has a job, as in
the app: state, identity (one tone per tab), or traffic direction (gold down, magenta up).

### Type

- **Figtree** (Google, variable, self-hosted by `next/font`) for everything. Weights 400 text,
  500 headings and UI, 600 display and buttons. **Nothing above 600**, including `strong`.
- **System mono** (`ui-monospace, Cascadia Mono, Consolas`) only for data the app also sets in
  mono: addresses, ports, file names, sizes, commands. No extra font download, and on Windows it
  is the same face the app uses.
- Scale: mega `clamp(3.4rem, 1.7rem + 6.6vw, 8.25rem)`, tracking -0.05em, leading 0.92; xl
  `clamp(2.5rem, 1.5rem + 3.9vw, 5.25rem)`, -0.04em; l `clamp(1.7rem, 1.2rem + 1.9vw, 3rem)`,
  -0.03em; m, body 16 to 18 px at 1.55, small UI 13 to 14 px.

### Grid and rhythm

12 columns inside a 1320 px frame, gutter `clamp(20px, 1rem + 1.5vw, 40px)`, 4 columns on
phones. Three vertical lines carry the text: column 1, column 6, column 8. Section padding
varies on purpose: hero (full screen), airy (26vh) for the statement, sticky (no padding, the
chapters set the pace) for features, normal (16vh) for the rest, a full-screen finale.

### Shape

One rule: **everything is round.** Every control, chip, badge, nav and input is a full pill
(999 px). Surfaces are 32 px (24 px on phones). Screens sit in a bezel whose radii are
concentric: outer 32, padding 10, inner 22.

### Motion language

- GSAP and Lenis own scroll and page choreography on one ticker. Motion owns the orb and the
  menu, whose state lives in React; the orb is ported from the app's own Motion code, with its
  press spring (stiffness 520, damping 30, mass 0.7).
- Eases: `expo.out` entrances, `expo.inOut` for the transition, `power3.out` for UI, the app's
  `cubic-bezier(0.32, 0.72, 0, 1)` for CSS.
- **Connect in practice:** text arrives along its line and locks (masked line reveals); screens
  open from an inset clip; packets travel the route diagram; the orb travels into the window and
  lands on the app's orb; the page transition is a ring that opens from where you clicked.
- One treatment per role. Primary pill: the label rolls and the icon travels along its
  direction. Secondary pill: a fill floods from the pointer's side and inverts the text. Nav: a
  pill marker slides to the hovered item (Motion `layoutId`), the current page keeps it. Rows
  and cards: the border lights in the state colour and the arrow travels. Inline links: an
  underline at rest that rises on hover. Utility links: colour only.
- **Idle state:** the connected ring turns slowly (as the app's rim does), its halo breathes,
  and packets keep flowing through the route diagram, all paused off screen. Off-state the ring
  holds still, because the tunnel is down.
- **Reduced motion:** native scroll, no reveals or entrances, no morph, no packets in flight
  (the diagram is drawn static), no breathing light, the orb starts connected and switches
  without a count, and transitions are short fades.

### Loader

None. A product page should be readable at once; the orb's connect count is the intro.

### Decisions made during QA

- **First screens enter in CSS, not GSAP.** Headlines rise out of a mask (`.rise`) and the rest
  fades up (`[data-enter]`) from the first paint, without waiting for JavaScript. The largest
  text (each page's lede) slides but never fades, so it counts as painted at once. Measured on
  the QA phone (4x CPU, 1.6 Mbps): LCP went from 3.8 s to 2.2 s. Below the fold, reveals stay
  on the engine (`data-split`, `data-reveal`). During a page transition the html carries
  `is-covered`, and the entrances replay as the ring opens.
- **The engine boots after the page hydrates.** The page segment hydrates after the layout, so
  every page ends with `<PageReady />` and the engine waits for it; booting earlier let
  SplitText rewrite text React had not hydrated, and React threw the page away.
- **Nothing may stop the clock.** A throw inside a `gsap.ticker` callback stops GSAP, Lenis and
  every transition at once, so the route packets run inside a try/catch, and the router
  adapter caps how long a leave can take and recovers a navigation that never commits.
- **Motion is loaded lazily** (`LazyMotion` + `domAnimation`, `m.*` components), and the nav
  marker is a CSS clip-path instead of a `layoutId`, which keeps the JS under 300 KB gzipped.
  The orb's count runs on GSAP's ticker and writes into the motion value.
- **Idle state on every page:** the room light breathes slowly (7 s), like the app's orb halo.
  On the home page the connected ring turns and packets flow through the route diagram.
- **The footer wordmark is SVG**, outline only. It is a logotype; as HTML text a contrast
  checker reads it as body copy at 1.05:1.
- **After the first outside critique (7.35):** the 404's orb is pressable and reconnects by
  going home; the hero CTAs never fade, so they are at full contrast from the first frame; the
  tunnel opens in about 2 s instead of 3; the badge keeps a word ("On", "Off") on narrow phones;
  phones show each product shot cropped to its detail at twice the size (`shot.focus`); code
  blocks carry their Copy button in a bar above the command; the header is opaque enough that
  nothing reads through it.

## Composition plan (each section a different family)

```
HERO (split)                                      1440
+-------------------------------------------------------------+
| (O) OpenWire   Features How it works Privacy Download  [TUNNEL ON] [GH] |  floating pill
|                                                             |
| [New in 0.2.0: connection logs ->]                          |
| See every byte.                         ( ring  )  Tunnel is on
| Tunnel it yourself.                     ( power )  Through your server
| sub copy, 2 to 3 lines                  caption: the app's button, working
| [Download for Windows  v0.2.0 . 5.1 MB] [GH View on GitHub] |
| Windows 10 and 11 | GPL-3.0 | No account                    |
|        +------------------- app window --------------------+|  rises, the orb lands on its orb
+-------------------------------------------------------------+

STATEMENT (manifesto, cols 1-10, words light up on scroll)

FEATURES (sticky narrative)
| chapter list, cols 1-5   |  sticky window, cols 6-12      |
| [tone] VPN ...           |  [ screen swaps per chapter ]  |
| [tone] Network ...       |  ( VPN Network Usage ... )     |  tab marker slides

HOW IT WORKS (full-bleed diagram)
| apps --- OpenWire 127.0.0.1:10808 === TLS === your server --- internet |
|   \________________ direct, when the tunnel is off ______________/      |
| 01 Inbound | 02 Routing | 03 Outbound | 04 System proxy   [tunnel switch] |

FIRST SERVER (asymmetric pair)
| big link pill being pasted, Ctrl + V keys | 3 steps, constraints |

PRIVACY (data table): what stays where, three rows; what it talks to

OPEN SOURCE (facts + index list): 5.1 MB, Rust + Tauri 2, GPL-3.0; roadmap rows

FOOTER (finale): "Open the tunnel." + CTAs, link columns, giant OpenWire wordmark
with the wire drawing under it, live "this visit" readout
```

Inner pages: `/download` (version poster, download rows, before you install, what's new from the
release notes, build from source), `/privacy` (the policy as a document), and the 404, where the
tunnel is pinned off: "Nothing at the other end."

## Genericness test

A generic model given this prompt lands on a centred hero over a gradient blob, three feature
cards, a floating screenshot and a logo strip. Changed:

1. The glow is not decoration. It is the app's ambient field, its colour is the tunnel state,
   and a click changes it across the whole site.
2. No feature cards. One sticky window swaps the real screens chapter by chapter, and the active
   tab moves inside the real UI.
3. The route diagram reacts to the same tunnel state: packets go through the tunnel, or
   straight out when it is off.
4. The hero orb hands off into the product window instead of floating beside it.
5. The 404 and the transition carry the concept (a tunnel that is off; a ring that opens).

## Assumptions and placeholders

- The production URL is unknown. `SITE_URL` sets it; on Vercel the production domain is picked
  up automatically. Until then canonical and OG URLs point at localhost.
- Release data comes from `api.github.com` at build time and is refreshed hourly. If GitHub
  cannot be reached, the buttons fall back to the latest-release page.
