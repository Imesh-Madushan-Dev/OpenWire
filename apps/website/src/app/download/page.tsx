import type { Metadata } from 'next';
import type { ReactNode } from 'react';
import { File01Icon, Package01Icon, WindowsNewIcon } from '@hugeicons/core-free-icons';
import type { IconSvgElement } from '@hugeicons/react';

import { CodeBlock } from '@/components/code-block';
import { Icon } from '@/components/icon';
import { DownloadButton, GhostButton } from '@/components/pills';
import { ReleaseNotes } from '@/components/release-notes';
import { site } from '@/data/site';
import { PageReady } from '@/engine/next';
import { delay } from '@/lib/enter';
import { getLatestRelease, type Asset } from '@/lib/release';

export const metadata: Metadata = {
  title: 'Download',
  description:
    'Download the latest OpenWire for Windows 10 and 11: the standard installer, an MSI for managed deployment, and checksums to verify them.',
};

function Row({
  icon,
  name,
  asset,
  use,
  lead = false,
  action,
}: {
  icon: IconSvgElement;
  name: string;
  asset: Asset;
  use: string;
  lead?: boolean;
  action: ReactNode;
}) {
  return (
    <li className={`dl-row${lead ? ' dl-row--lead' : ''}`} data-reveal="up">
      <span className="dl-row__icon" aria-hidden="true">
        <Icon icon={icon} />
      </span>
      <div className="dl-row__text">
        <h3 className="dl-row__name">
          {name}
          {lead && <span className="tag tag--state">Recommended</span>}
        </h3>
        <p className="dl-row__file">
          {asset.name}, {asset.sizeLabel}
        </p>
        <p className="dl-row__use">{use}</p>
      </div>
      {action}
    </li>
  );
}

export default async function DownloadPage() {
  const release = await getLatestRelease();
  const { installer, msi, checksums } = release;
  const hashCommand = `Get-FileHash .\\${installer?.name ?? `OpenWire_${release.version}_x64-setup.exe`} -Algorithm SHA256`;

  return (
    <>
      <section className="page-hero" aria-labelledby="download-title">
        <div className="frame page-hero__grid">
          <div className="page-hero__copy">
            <h1 id="download-title" className="page-hero__title">
              <span className="rise" style={delay(0.06)}>
                <span>Download OpenWire</span>
              </span>
            </h1>
            <p className="lede" style={{ maxWidth: '44ch', ...delay(0.26) }} data-enter="slide">
              Version {release.version}, released {release.dateLabel}. For Windows 10 and 11, 64-bit. Free, with
              nothing to sign up for.
            </p>
          </div>
          <p className="version-mark" aria-hidden="true" data-enter style={delay(0.14)}>
            {release.version}
          </p>
        </div>
      </section>

      <section className="frame" aria-labelledby="files-title">
        <h2 id="files-title" className="sr-only">
          Files for version {release.version}
        </h2>
        {installer ? (
          <ul className="downloads" style={{ listStyle: 'none', padding: 0 }}>
            <Row
              icon={WindowsNewIcon}
              name="Installer"
              asset={installer}
              use="The standard installer. It offers detailed monitoring during setup."
              lead
              action={<DownloadButton release={release} size="md" label="Download" />}
            />
            {msi && (
              <Row
                icon={Package01Icon}
                name="MSI package"
                asset={msi}
                use="For managed deployment with Intune or Group Policy. Turn on detailed monitoring later in Settings › Monitoring."
                action={<GhostButton href={msi.url} label="Download .msi" size="md" external={false} />}
              />
            )}
            {checksums && (
              <Row
                icon={File01Icon}
                name="Checksums"
                asset={checksums}
                use="SHA-256 hashes for both files, to confirm what you downloaded is the genuine build."
                action={<GhostButton href={checksums.url} label="View checksums" size="md" />}
              />
            )}
          </ul>
        ) : (
          <div className="actions">
            <DownloadButton release={release} label="Get it from GitHub" meta={false} />
          </div>
        )}
      </section>

      <div className="frame doc">
        <section className="doc__block" aria-labelledby="before-title">
          <h2 id="before-title" data-split="lines">
            Before you install
          </h2>
          <div className="doc__body" data-reveal="up">
            <p>
              <strong>Windows SmartScreen may warn you.</strong> This release is not code-signed yet, so Windows shows
              “Windows protected your PC”. Choose <strong>More info</strong>, then <strong>Run anyway</strong>. To
              confirm the file is genuine first, compare its hash with the checksums file:
            </p>
            <CodeBlock code={hashCommand} shell="PowerShell" label="PowerShell command to hash the installer" />
            <p>
              <strong>Administrator approval is needed once</strong>, because OpenWire installs for all users and can
              set up its monitoring service.
            </p>
          </div>
        </section>

        <section className="doc__block" aria-labelledby="needs-title">
          <h2 id="needs-title" data-split="lines">
            What it needs
          </h2>
          <div className="doc__body" data-reveal="up">
            <ul>
              <li>Windows 10 or 11, 64-bit.</li>
              <li>
                The Microsoft Edge WebView2 runtime. Windows 11 already has it, and the installer sets it up if it is
                missing.
              </li>
              <li>A VLESS or Trojan server to tunnel through. The monitor works without one.</li>
            </ul>
            <p>
              <strong>Upgrading?</strong> Run the new installer. Your servers, settings and usage history are kept.
            </p>
          </div>
        </section>

        {release.notes && (
          <section className="doc__block" aria-labelledby="new-title">
            <h2 id="new-title" data-split="lines">
              What’s new in {release.version}
            </h2>
            <div className="doc__body" data-reveal="up">
              <ReleaseNotes markdown={release.notes} />
              <p>
                <a className="link" href={release.url} target="_blank" rel="noopener noreferrer">
                  The full release notes on GitHub
                </a>
              </p>
            </div>
          </section>
        )}

        <section className="doc__block" aria-labelledby="build-title">
          <h2 id="build-title" data-split="lines">
            Build it yourself
          </h2>
          <div className="doc__body" data-reveal="up">
            <p>
              You need <a className="link" href="https://bun.sh">Bun</a> 1.2 or later,{' '}
              <a className="link" href="https://rustup.rs">Rust</a> (stable, MSVC toolchain) and the{' '}
              <a className="link" href="https://tauri.app/start/prerequisites/">
                Tauri prerequisites for Windows
              </a>
              .
            </p>
            <CodeBlock
              shell="Terminal"
              label="Commands to build OpenWire from source"
              code={[
                `git clone ${site.repo}.git`,
                'cd OpenWire',
                'bun install',
                '',
                'cd apps/desktop-app',
                'bun run tauri:dev      # run the app with hot reload',
                'bun run tauri:build    # build the installer',
              ].join('\n')}
            />
            <p>
              Every earlier version is on the{' '}
              <a className="link" href={site.releases} target="_blank" rel="noopener noreferrer">
                releases page
              </a>
              .
            </p>
          </div>
        </section>
      </div>
      <PageReady />
    </>
  );
}
