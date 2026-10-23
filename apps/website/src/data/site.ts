// Every visible string on the site lives here, so copy changes never touch markup.
// Facts come from the repository README and the 0.2.0 release notes. Nothing is invented:
// the product images are the real UI running on the demo's made-up data, and the version,
// file names and sizes are read from GitHub at build time (src/lib/release.ts).
//
// Assumptions (mirrored in DESIGN.md):
// - The production URL is unknown; set SITE_URL when deploying (src/lib/site-url.ts).

export const REPO = 'https://github.com/Imesh-Madushan-Dev/OpenWire';

export const site = {
  name: 'OpenWire',
  title: 'OpenWire: network monitor and tunnel for Windows',
  description:
    'A free, open-source network monitor and tunnel for Windows. See every app, host and device on your network, and route traffic through your own server.',
  author: 'Imesh Madushan',
  year: 2026,
  repo: REPO,
  releases: `${REPO}/releases`,
  latest: `${REPO}/releases/latest`,
  issues: `${REPO}/issues`,
  newIssue: `${REPO}/issues/new`,
  license: `${REPO}/blob/main/LICENSE`,
  developerGuide: `${REPO}/blob/main/apps/desktop-app/README.md`,
};

export const nav = [
  { label: 'Features', href: '/#features' },
  { label: 'How it works', href: '/#how' },
  { label: 'Privacy', href: '/privacy' },
];

export const hero = {
  titleLead: 'See every byte.',
  titleTunnel: 'Tunnel it yourself.',
  lede: 'OpenWire is a free, open-source network monitor and tunnel for Windows. ',
  download: 'Download for Windows',
  repo: 'View on GitHub',
  facts: ['Windows 10 and 11, 64-bit', 'GPL-3.0', 'No account, no telemetry'],
  windowAlt:
    'OpenWire’s VPN tab in the Graphite theme: the tunnel is on through a server in Singapore, with six servers listed by latency',
};

/** A product named in the statement and the table, in its own colour: a site tone, or the state. */
export type Named = { name: string; tone: Tone | 'state' };

const glasswire: Named = { name: 'GlassWire', tone: 'amber' };
const netmod: Named = { name: 'NetMod VPN', tone: 'violet' };
const openwire: Named = { name: 'OpenWire', tone: 'state' };

export const statement: (string | Named)[] = [
  glasswire,
  ' monitors your traffic, but it is paid. ',
  netmod,
  ' tunnels it, but does not monitor it. ',
  openwire,
  ' does both, free and open source.',
];

/**
 * The difference, side by side. Checked against glasswire.com/pricing and NetMod's download
 * listings (SourceForge, MajorGeeks) in October 2026: keep it to what those pages say.
 */
export const compare = {
  products: [glasswire, netmod, openwire],
  rows: [
    { label: 'Live traffic by app, host and device', values: [true, false, true] },
    { label: 'Tunnel through your own VLESS or Trojan server', values: [false, true, true] },
    { label: 'Price', values: ['Paid', 'Free', 'Free'] },
    { label: 'Licence', values: ['Closed source', 'Freeware', 'Open source, GPL-3.0'] },
  ],
  note: 'GlassWire also has a free tier, limited to 24 hours of history and without its firewall controls; Premium is billed yearly. As listed by each project in October 2026.',
} satisfies { products: Named[]; rows: { label: string; values: (boolean | string)[] }[]; note: string };

export type Tone = 'sky' | 'amber' | 'violet' | 'green' | 'rose';

export type Chapter = {
  id: string;
  tab: string;
  tone: Tone;
  title: string;
  body: string;
  points?: string[];
  shot: { src: string; alt: string };
};

export const features = {
  title: 'One window for your whole network.',
  lede: 'The same five tabs you get after you install. Every picture here is the real app.',
  chapters: [
    {
      id: 'vpn',
      tab: 'VPN',
      tone: 'sky',
      title: 'A tunnel to your own server.',
      body: 'Paste a vless:// or trojan:// link and press the power button. OpenWire runs a local proxy and points Windows at it, so every app that uses the system proxy goes through the tunnel, wrapped in TLS with a server name you choose.',
      points: [
        'A kill switch that blocks traffic if the server drops',
        'Printers and NAS drives stay reachable on your local network',
        'The fastest server, picked the moment you connect',
        'Your proxy settings put back on disconnect, on exit, even after a crash',
      ],
      shot: { src: '/screens/vpn.webp', alt: hero.windowAlt },
    },
    {
      id: 'network',
      tab: 'Network',
      tone: 'amber',
      title: 'A live graph of everything.',
      body: 'Download in gold, upload in magenta, updated every second. Zoom out from five minutes to three hours, a day, a week or a month, or filter the graph to only the traffic that went through the tunnel.',
      shot: {
        src: '/screens/network.webp',
        alt: 'OpenWire’s Network tab: a live five-minute graph of download in gold and upload in magenta, with an overview strip underneath',
      },
    },
    {
      id: 'usage',
      tab: 'Usage',
      tone: 'violet',
      title: 'Know which app used it.',
      body: 'Apps ranked by data, the hosts they talked to with the country each one is in, and the kind of traffic it was: HTTPS, QUIC, streaming, real-time, file transfer. Browse any day, week or month.',
      points: [
        'Detailed monitoring reads Windows’ own network events through a small background service',
        'No kernel driver, and never the contents of your traffic',
      ],
      shot: {
        src: '/screens/usage.webp',
        alt: 'OpenWire’s Usage tab: 4.1 GB in a day, ranked by app, by host with country flags, and by traffic type',
      },
    },
    {
      id: 'devices',
      tab: 'Devices',
      tone: 'green',
      title: 'See who is on your Wi-Fi.',
      body: 'Scan your Wi-Fi or Ethernet network for every connected device, with its name, maker, type and when it was first seen. OpenWire can keep watching in the background and tell you when a new one joins.',
      shot: {
        src: '/screens/devices.webp',
        alt: 'OpenWire’s Devices tab: twelve devices on a home network, from the router to a PlayStation, with maker, status and first-seen date',
      },
    },
    {
      id: 'themes',
      tab: 'Settings',
      tone: 'rose',
      title: 'Make it look like yours.',
      body: 'Neon, Aurora and Graphite, each in light and dark, plus five palettes for the charts. Live upload and download speed sits on the Windows taskbar, just left of the clock, and opens OpenWire with a click.',
      shot: {
        src: '/screens/settings.webp',
        alt: 'OpenWire’s Appearance settings: the Graphite, Aurora and Neon themes, and five chart colour palettes',
      },
    },
  ] satisfies Chapter[],
};

export const how = {
  title: 'How the tunnel works.',
  lede: 'Four moving parts, all on your PC until the last one.',
  nodes: {
    apps: { label: 'Your apps', detail: 'Anything that uses the system proxy' },
    openwire: { label: 'OpenWire', detail: '127.0.0.1:10808' },
    tunnel: 'VLESS or Trojan, inside TLS',
    server: { label: 'Your server', detail: 'The one you run' },
    internet: { label: 'The internet', detail: 'Sees your server, not you' },
    direct: 'Direct, while the tunnel is off',
  },
  switchCaption: 'This switch and the button at the top are the same tunnel.',
  steps: [
    {
      title: 'Inbound',
      text: 'One local listener speaks both SOCKS5 and HTTP, bound to 127.0.0.1 only, so it can never become an open relay.',
    },
    {
      title: 'Routing',
      text: 'Local network and loopback addresses can skip the tunnel. Everything else goes through it.',
    },
    {
      title: 'Outbound',
      text: 'A TLS connection to your server carries the real destination inside an encrypted VLESS or Trojan stream.',
    },
    {
      title: 'System proxy',
      text: 'Windows points at the local listener while you are connected, and goes back exactly as it was afterwards.',
    },
  ],
};

export const firstServer = {
  title: 'Bring your own server.',
  lede: 'OpenWire does not sell servers. Point it at one you run, or one a friend shares with you.',
  steps: [
    'Copy your server’s vless:// or trojan:// link.',
    'Press Ctrl+V on the VPN tab, or click Add. A screenshot of a QR code works too.',
    'Click the power button.',
  ],
  note: 'Supported links use TLS over TCP. REALITY, XTLS, WebSocket, gRPC and HTTPUpgrade links are turned away at import, with a message saying why.',
  example: 'vless://7f3c…e21a@203.0.113.24:443?security=tls&sni=cdn.yours.example#Tokyo',
  exampleCaption: 'An example link: 203.0.113.x addresses and .example names are set aside for documentation.',
  added: { name: 'Tokyo', host: '203.0.113.24', latency: '68 ms' },
};

export const privacy = {
  title: 'Your data stays on your PC.',
  lede: 'No accounts, no analytics, no telemetry. Beyond the servers you add, OpenWire talks to two things, and you can switch one of them off.',
  talksTo: [
    { what: 'Devices on your own network', when: 'Only while it scans them.' },
    {
      what: 'GitHub, to see if a release is out',
      when: 'Twice a day, carrying nothing about you. Turn it off in Settings › General.',
    },
  ],
  rows: [
    { what: 'Servers, settings, usage history, device list', where: '%APPDATA%\\com.openwire.desktop' },
    { what: 'IP-to-country lookups', where: 'A database built into the app' },
    { what: 'Detailed monitoring', where: 'Windows’ own events, read on your PC' },
  ],
  more: 'Read the privacy page',
};

export const openSource = {
  title: 'Built in the open.',
  lede: 'Free software under the GPL. A Rust core and the WebView2 window Windows already has, with no bundled browser engine, which is how the whole installer stays this small.',
  facts: [
    { value: 'GPL-3.0', label: 'Use it, study it, share it, change it' },
    { value: '0', label: 'Accounts, analytics or tracking calls' },
  ],
  roadmapTitle: 'Next on the list',
  roadmap: [
    { name: 'Firewall', text: 'Allow or block each app from reaching the network.' },
    { name: 'TUN mode', text: 'A virtual network adapter, for apps that ignore the system proxy.' },
    { name: 'More transports', text: 'REALITY, WebSocket and gRPC.' },
    { name: 'macOS and Linux routing', text: 'The tunnel already runs there; the system wiring does not yet.' },
  ],
  source: 'View the source',
  issue: 'Open an issue',
};

export const finale = {
  title: 'Open the tunnel.',
  lede: 'Free for Windows 10 and 11. No account, nothing to sign up for.',
};

export const colophon = 'Set in Figtree. Icons by Hugeicons. Built with Next.js, GSAP, Lenis and Motion.';
