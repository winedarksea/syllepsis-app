import { useEffect, useMemo, useRef, useState } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { api } from '../lib/api';
import { sanitizeHtml } from '../lib/sanitize';

interface Props {
  markdown: string;
  className?: string;
  findPattern?: string;
  findMatchIndex?: number;
  onMatchCount?: (count: number) => void;
}

export function MarkdownRenderer({ markdown, className, findPattern, findMatchIndex = 0, onMatchCount }: Props) {
  const [html, setHtml] = useState('');
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    api.renderNoteMarkdown({ markdown })
      .then((rendered) => { if (!cancelled) setHtml(sanitizeHtml(rendered)); })
      .catch(() => { if (!cancelled) setHtml(''); });
    return () => { cancelled = true; };
  }, [markdown]);

  const { highlightedHtml, matchCount } = useMemo(() => {
    const pattern = findPattern ?? '';
    if (!pattern.trim()) {
      onMatchCount?.(0);
      return { highlightedHtml: html, matchCount: 0 };
    }
    try {
      const regex = new RegExp(pattern, 'g');
      let index = 0;
      const highlighted = html.replace(regex, (match) => {
        const active = index === findMatchIndex;
        index += 1;
        return `<mark class="${active ? 'note-find-hit active' : 'note-find-hit'}">${match}</mark>`;
      });
      onMatchCount?.(index);
      return { highlightedHtml: highlighted, matchCount: index };
    } catch {
      onMatchCount?.(0);
      return { highlightedHtml: html, matchCount: 0 };
    }
    // onMatchCount is excluded intentionally — it's a callback ref, not reactive data
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [html, findPattern, findMatchIndex]);

  // Scroll the active match into view whenever it changes
  useEffect(() => {
    if (!matchCount || !containerRef.current) return;
    const active = containerRef.current.querySelector<HTMLElement>('.note-find-hit.active');
    active?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }, [highlightedHtml, matchCount]);

  return (
    <div
      ref={containerRef}
      className={className}
      dangerouslySetInnerHTML={{ __html: highlightedHtml }}
      onClick={(event) => {
        const target = event.target as HTMLElement | null;
        const cloze = target?.closest<HTMLButtonElement>('.syl-cloze');
        if (cloze) {
          cloze.classList.toggle('revealed');
          cloze.textContent = cloze.classList.contains('revealed')
            ? cloze.dataset.hidden ?? ''
            : cloze.textContent || 'show';
          return;
        }
        const link = target?.closest<HTMLAnchorElement>('a[href]');
        const href = link?.getAttribute('href');
        if (!href) return;
        if (/^(https?:|mailto:)/i.test(href)) {
          event.preventDefault();
          void openUrl(href);
        }
      }}
    />
  );
}
