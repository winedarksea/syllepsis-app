import { useEffect, useMemo, useState } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { api } from '../lib/api';
import { sanitizeHtml } from '../lib/sanitize';

interface Props {
  markdown: string;
  className?: string;
  findPattern?: string;
  findMatchIndex?: number;
}

export function MarkdownRenderer({ markdown, className, findPattern, findMatchIndex = 0 }: Props) {
  const [html, setHtml] = useState('');

  useEffect(() => {
    let cancelled = false;
    api.renderNoteMarkdown({ markdown })
      .then((rendered) => { if (!cancelled) setHtml(sanitizeHtml(rendered)); })
      .catch(() => { if (!cancelled) setHtml(''); });
    return () => { cancelled = true; };
  }, [markdown]);

  const highlightedHtml = useMemo(() => {
    const pattern = findPattern ?? '';
    if (!pattern.trim()) return html;
    try {
      const regex = new RegExp(pattern, 'g');
      let index = 0;
      return html.replace(regex, (match) => {
        const active = index === findMatchIndex;
        index += 1;
        return `<mark class="${active ? 'note-find-hit active' : 'note-find-hit'}">${match}</mark>`;
      });
    } catch {
      return html;
    }
  }, [findMatchIndex, findPattern, html]);

  return (
    <div
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
