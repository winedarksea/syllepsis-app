import { useEffect, useRef, useState } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import { sanitizeHtml } from '../lib/sanitize';
import { findLiteralMatches } from '../editor/find';

interface Props {
  markdown: string;
  className?: string;
  findPattern?: string;
  findMatchIndex?: number;
  onMatchCount?: (count: number) => void;
}

export function MarkdownRenderer({ markdown, className, findPattern, findMatchIndex = 0, onMatchCount }: Props) {
  const { openEditor } = useStore();
  const [html, setHtml] = useState('');
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    api.renderNoteMarkdown({ markdown })
      .then((rendered) => { if (!cancelled) setHtml(sanitizeHtml(rendered)); })
      .catch(() => { if (!cancelled) setHtml(''); });
    return () => { cancelled = true; };
  }, [markdown]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    container.innerHTML = html;
    const matchCount = highlightLiteralMatches(container, findPattern ?? '', findMatchIndex);
    onMatchCount?.(matchCount);
    if (!matchCount) return;

    const active = container.querySelector<HTMLElement>('.note-find-hit.active');
    active?.scrollIntoView({ block: 'center', inline: 'nearest', behavior: 'smooth' });
  }, [findMatchIndex, findPattern, html, onMatchCount]);

  return (
    <div
      ref={containerRef}
      className={className}
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
        if (href.startsWith('syllepsis://note/')) {
          event.preventDefault();
          const id = href.slice('syllepsis://note/'.length);
          if (id) openEditor(id);
          return;
        }
        if (/^(https?:|mailto:)/i.test(href)) {
          event.preventDefault();
          void openUrl(href);
        }
      }}
    />
  );
}

function highlightLiteralMatches(container: HTMLElement, pattern: string, activeIndex: number): number {
  const textNodes = collectTextNodes(container);
  const fullText = textNodes.map((node) => node.data).join('');
  const matches = findLiteralMatches(fullText, pattern);
  if (matches.length === 0) return 0;

  const textNodeRanges: Array<{ node: Text; start: number; end: number }> = [];
  let cursor = 0;
  for (const node of textNodes) {
    const start = cursor;
    const end = start + node.data.length;
    textNodeRanges.push({ node, start, end });
    cursor = end;
  }

  // Both `matches` and `textNodeRanges` are already in document order, so a single advancing
  // pointer pairs each range with its overlapping matches in O(matches + nodes) instead of
  // re-filtering the entire match list for every text node.
  let matchIndex = 0;
  for (const range of textNodeRanges) {
    // A match that ended before this range started can never overlap any later range either
    // (ranges only move forward), so it is safe to retire for good.
    while (matchIndex < matches.length && matches[matchIndex].end <= range.start) {
      matchIndex++;
    }
    const segments: Array<{ start: number; end: number; active: boolean }> = [];
    for (let i = matchIndex; i < matches.length && matches[i].start < range.end; i++) {
      const match = matches[i];
      if (range.end > match.start && range.start < match.end) {
        segments.push({
          start: Math.max(0, match.start - range.start),
          end: Math.min(range.node.data.length, match.end - range.start),
          active: i === activeIndex,
        });
      }
    }
    wrapTextNodeRanges(range.node, segments);
  }

  return matches.length;
}

function collectTextNodes(root: HTMLElement): Text[] {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (node) => node.textContent ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT,
  });
  const nodes: Text[] = [];
  let node = walker.nextNode();
  while (node) {
    nodes.push(node as Text);
    node = walker.nextNode();
  }
  return nodes;
}

function wrapTextNodeRanges(node: Text, segments: Array<{ start: number; end: number; active: boolean }>) {
  if (segments.length === 0 || !node.parentNode) return;

  const text = node.data;
  const fragment = document.createDocumentFragment();
  let cursor = 0;

  for (const segment of segments) {
    if (segment.start >= segment.end) continue;
    if (segment.start > cursor) {
      fragment.append(document.createTextNode(text.slice(cursor, segment.start)));
    }

    const mark = document.createElement('mark');
    mark.className = segment.active ? 'note-find-hit active' : 'note-find-hit';
    mark.textContent = text.slice(segment.start, segment.end);
    fragment.append(mark);
    cursor = segment.end;
  }

  if (cursor < text.length) fragment.append(document.createTextNode(text.slice(cursor)));
  node.parentNode.replaceChild(fragment, node);
}
