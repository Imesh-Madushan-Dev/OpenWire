'use client';

import { useState } from 'react';
import { AnimatePresence } from 'motion/react';
import * as m from 'motion/react-m';
import { swapVariants } from '@/lib/motion';

/** A command to run, under a bar naming the shell, with a copy button that says when it worked. */
export function CodeBlock({ code, label, shell }: { code: string; label: string; shell: string }) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch {
      // Clipboard blocked: the text is still selectable.
    }
  };

  return (
    <div className="code">
      <div className="code__bar">
        <span className="code__shell">{shell}</span>
        <button type="button" className="pill pill--ghost" data-fill onClick={copy} aria-label={`Copy: ${label}`}>
          <span className="pill__fill" aria-hidden="true" />
          <AnimatePresence mode="wait" initial={false}>
            <m.span key={copied ? 'done' : 'copy'} variants={swapVariants} initial="initial" animate="animate" exit="exit">
              {copied ? 'Copied' : 'Copy'}
            </m.span>
          </AnimatePresence>
        </button>
      </div>
      <pre data-lenis-prevent tabIndex={0} aria-label={label}>
        <code>{code}</code>
      </pre>
      <span className="sr-only" role="status">
        {copied ? 'Copied to the clipboard' : ''}
      </span>
    </div>
  );
}
