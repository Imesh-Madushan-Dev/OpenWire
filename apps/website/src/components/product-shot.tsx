import Image from 'next/image';

import { ScreenViewer } from './screen-viewer';

/**
 * The real app in a rounded bezel. Images are the README's 820x520 windows at 2x, copied from
 * docs/screenshots (`bun run screens`), so their proportions are fixed at 1640 / 1040. Shown whole
 * on every screen; a tap opens it full size.
 */
export function ProductShot({
  src,
  alt,
  sizes,
  eager = false,
  className = '',
  reveal,
}: {
  src: string;
  alt: string;
  sizes: string;
  eager?: boolean;
  className?: string;
  reveal?: boolean;
}) {
  return (
    <figure className={`window ${className}`} {...(reveal ? { 'data-reveal': 'clip' } : {})}>
      <div className="window__bezel">
        <div className="window__screen">
          <Image
            src={src}
            alt={alt}
            width={1640}
            height={1040}
            sizes={sizes}
            quality={90}
            loading={eager ? 'eager' : 'lazy'}
            data-full={src}
          />
          <ScreenViewer />
        </div>
      </div>
    </figure>
  );
}
