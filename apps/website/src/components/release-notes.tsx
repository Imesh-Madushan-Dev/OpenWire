import { Fragment, type ReactNode } from 'react';

/*
 * Renders the "What's new" part of a GitHub release body: ### headings, paragraphs and - bullets,
 * with **bold**, `code` and [links](https://...) inline. Everything else stays plain text: the
 * notes come from GitHub, so nothing in them is ever treated as HTML.
 */

const INLINE = /(\*\*([^*]+)\*\*|`([^`]+)`|\[([^\]]+)\]\((https?:\/\/[^)\s]+)\))/g;

function inline(text: string): ReactNode[] {
  const out: ReactNode[] = [];
  let last = 0;
  for (const match of text.matchAll(INLINE)) {
    const at = match.index ?? 0;
    if (at > last) out.push(text.slice(last, at));
    if (match[2]) out.push(<strong key={at}>{match[2]}</strong>);
    else if (match[3]) out.push(<code key={at}>{match[3]}</code>);
    else if (match[4] && match[5])
      out.push(
        <a key={at} className="link" href={match[5]} target="_blank" rel="noopener noreferrer">
          {match[4]}
        </a>,
      );
    last = at + match[0].length;
  }
  if (last < text.length) out.push(text.slice(last));
  // GitHub's notes use " - " as a dash; this site writes without them.
  return out.map((part) => (typeof part === 'string' ? part.replace(/\s+-\s+/g, ': ') : part));
}

type Block = { kind: 'h3'; text: string } | { kind: 'p'; text: string } | { kind: 'ul'; items: string[] };

function parse(markdown: string): Block[] {
  const blocks: Block[] = [];
  let paragraph: string[] = [];
  const flush = () => {
    if (paragraph.length) blocks.push({ kind: 'p', text: paragraph.join(' ') });
    paragraph = [];
  };
  for (const raw of markdown.split('\n')) {
    const line = raw.trim();
    if (!line) {
      flush();
    } else if (line.startsWith('### ')) {
      flush();
      blocks.push({ kind: 'h3', text: line.slice(4) });
    } else if (/^[-*]\s+/.test(line)) {
      flush();
      const item = line.replace(/^[-*]\s+/, '');
      const prev = blocks[blocks.length - 1];
      if (prev?.kind === 'ul') prev.items.push(item);
      else blocks.push({ kind: 'ul', items: [item] });
    } else if (!line.startsWith('#') && !line.startsWith('|') && !line.startsWith('```')) {
      paragraph.push(line);
    }
  }
  flush();
  return blocks;
}

export function ReleaseNotes({ markdown }: { markdown: string }) {
  return (
    <>
      {parse(markdown).map((block, i) => (
        <Fragment key={i}>
          {block.kind === 'h3' && <h3>{inline(block.text)}</h3>}
          {block.kind === 'p' && <p>{inline(block.text)}</p>}
          {block.kind === 'ul' && (
            <ul>
              {block.items.map((item, j) => (
                <li key={j}>{inline(item)}</li>
              ))}
            </ul>
          )}
        </Fragment>
      ))}
    </>
  );
}
