# OpenWire website

The product site for OpenWire: a home page, a download page, a privacy page and a 404. Next.js
(App Router) in the Bun + Turbo monorepo, with GSAP, Lenis and Motion. `DESIGN.md` explains the
concept and every design decision.

## Run it

From the repository root, or from `apps/website`:

```bash
bun install
cd apps/website
bun run dev        # http://localhost:3000
bun run build      # production build
bun run start      # serve the production build
```

`bun run build` at the repository root builds it with the desktop app through Turbo.

## Configuration

| Variable | What it does |
| --- | --- |
| `SITE_URL` | The production origin, for canonical URLs, the sitemap and Open Graph. Vercel's production domain is used if it is not set. |
| `GITHUB_TOKEN` | Optional. Raises GitHub's API rate limit for the release lookup. |

The version, file names, sizes and release notes come from GitHub's latest release
(`src/lib/release.ts`), are cached for an hour, and need no redeploy when a new version ships.

## Where things are

| Path | What it holds |
| --- | --- |
| `src/data/site.ts` | Every visible string |
| `src/app/` | Pages, metadata, sitemap, robots, icon |
| `src/components/` | The header, footer, power orb, pills and home sections |
| `src/motion/` | The tunnel driver (`tunnel.ts`), the page transition and scroll choreography |
| `src/engine/` | The motion engine: one GSAP clock for Lenis, ScrollTrigger and the page lifecycle |
| `public/screens/` | Product shots of the real app, copied from `docs/screenshots` |

## Product shots

The images are the README's dark screenshots: the desktop app's real UI in the Graphite theme,
running on its made-up demo data. `docs/screenshots` is the one source. After regenerating them
there (`bun scripts/screenshots.ts` in `apps/desktop-app`), copy them into the site:

```bash
cd apps/website
bun run screens
```

`public/og.png`, the social preview, is a screenshot of the hero at 1200x630. Retake it when the
hero changes.
