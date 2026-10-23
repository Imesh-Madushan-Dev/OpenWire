import Image from 'next/image';
import type { CSSProperties } from 'react';

/**
 * The real app in a rounded bezel. Images are the README's 820x520 windows at 2x, copied from
 * docs/screenshots (`bun run screens`), so their proportions are fixed at 1640 / 1040.
 */
export function ProductShot({
  src,
  alt,
  sizes,
  eager = false,
  className = '',
  reveal,
  focus,
}: {
  src: string;
  alt: string;
  sizes: string;
  eager?: boolean;
  className?: string;
  reveal?: boolean;
  /** Where a cropped (phone) view of this window looks, as an object-position. */
  focus?: string;
}) {
  return (
    <figure
      className={`window ${className}`}
      style={focus ? ({ '--focus': focus } as CSSProperties) : undefined}
      {...(reveal ? { 'data-reveal': 'clip' } : {})}
    >
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
          />
        </div>
      </div>
    </figure>
  );
}
