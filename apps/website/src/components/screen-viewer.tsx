'use client';

import { useRef, useState } from 'react';
import { Cancel01Icon, Maximize02Icon } from '@hugeicons/core-free-icons';

import { startScroll, stopScroll } from '@/engine';
import { Icon } from './icon';

type Shot = { src: string; alt: string };

/**
 * A product window's screen that opens at full size. The screenshots are the app's 820x520
 * window at 2x, so on a phone the page shows them small: a tap opens the screen on its own, at the
 * app's real size on a phone (pan to see the rest) and as large as fits on a desktop.
 *
 * It sits over the screen as its own button, so the images keep their alt text. A screen holds
 * one image, or a stack where the stage's tab bar says which one is showing. Each image carries
 * `data-full`, the untouched file, so the viewer is sharp at any zoom.
 */
export function ScreenViewer() {
  const dialog = useRef<HTMLDialogElement>(null);
  const [shot, setShot] = useState<Shot | null>(null);

  const open = (button: HTMLElement) => {
    const screen = button.parentElement!;
    const images = Array.from(screen.querySelectorAll<HTMLImageElement>('img[data-full]'));
    const tab = screen.closest('.stage')?.querySelector<HTMLElement>('[data-tab][aria-current="true"]');
    const image = images.find((img) => img.dataset.shot === tab?.dataset.tab) ?? images[0];
    if (!image) return;
    setShot({ src: image.dataset.full!, alt: image.alt });
    dialog.current?.showModal();
    stopScroll();
  };

  const close = () => dialog.current?.close();

  return (
    <>
      <button
        type="button"
        className="screen-viewer"
        aria-haspopup="dialog"
        aria-label="View this screen full size"
        onClick={(event) => open(event.currentTarget)}
      >
        <span className="screen-viewer__hint" aria-hidden="true">
          <Icon icon={Maximize02Icon} size={16} />
        </span>
      </button>

      <dialog
        ref={dialog}
        className="viewer"
        aria-label={shot?.alt ?? 'Screen'}
        onClose={() => {
          startScroll();
          setShot(null);
        }}
        // A click anywhere but the screen itself closes, as the backdrop would.
        onClick={(event) => {
          if (!(event.target instanceof HTMLImageElement)) close();
        }}
      >
        <div className="viewer__scroll" data-lenis-prevent>
          {shot && (
            // The untouched file at its own size: the optimiser would only resize it.
            // eslint-disable-next-line @next/next/no-img-element
            <img className="viewer__img" src={shot.src} alt={shot.alt} width={1640} height={1040} />
          )}
        </div>
        <button type="button" className="icon-btn viewer__close" onClick={close} aria-label="Close" autoFocus>
          <Icon icon={Cancel01Icon} />
        </button>
      </dialog>
    </>
  );
}
