// Renders the sorted book as a continuous document (headings + paragraphs/bullets).
// Export buttons let users save the book as Markdown or HTML.

import { useCallback, useEffect, useState } from 'react';
import { save as saveDialog } from '@tauri-apps/plugin-dialog';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import { MarkdownRenderer } from '../components/MarkdownRenderer';
import { detectAccidentalWholeNoteCodeFence } from '../lib/wholeNoteFence';
import type { RenderItem } from '../types';
import './BookView.css';

// ── HeadingTag ───────────────────────────────────────────────────────────────

function HeadingTag({ level, text, id, onClick }: { level: number; text: string; id: string; onClick?: () => void }) {
  const l = Math.min(Math.max(level, 1), 6);
  const cls = `bv-heading bv-h${l}${onClick ? ' bv-heading-link' : ''}`;
  const inner = onClick
    ? <span className="bv-heading-text" onClick={onClick} role="button" tabIndex={0} onKeyDown={(e) => e.key === 'Enter' && onClick()}>{text}</span>
    : text;
  if (l === 1) return <h1 id={id} className={cls}>{inner}</h1>;
  if (l === 2) return <h2 id={id} className={cls}>{inner}</h2>;
  if (l === 3) return <h3 id={id} className={cls}>{inner}</h3>;
  if (l === 4) return <h4 id={id} className={cls}>{inner}</h4>;
  if (l === 5) return <h5 id={id} className={cls}>{inner}</h5>;
  return <h6 id={id} className={cls}>{inner}</h6>;
}

// ── headingId — stable DOM id for a heading ──────────────────────────────────

function headingId(category: string, index: number): string {
  return category
    ? `bv-h-${category.replace(/\s+/g, '-').toLowerCase()}`
    : `bv-h-${index}`;
}

// ── BookToc ───────────────────────────────────────────────────────────────────

interface TocEntry { id: string; level: number; text: string }

function BookToc({ headings }: { headings: TocEntry[] }) {
  const scrollTo = useCallback((id: string, e: React.MouseEvent) => {
    e.preventDefault();
    document.getElementById(id)?.scrollIntoView({ behavior: 'smooth' });
  }, []);

  return (
    <nav className="bv-toc" aria-label="Table of contents">
      <div className="bv-toc-title">Contents</div>
      <ul>
        {headings.map((h) => (
          <li key={h.id} className={`bv-toc-item bv-toc-h${h.level}`}>
            <a href={`#${h.id}`} onClick={(e) => scrollTo(h.id, e)}>
              {h.text}
            </a>
          </li>
        ))}
      </ul>
    </nav>
  );
}

// ── BookView ─────────────────────────────────────────────────────────────────

export function BookView() {
  const { openEditor, book, setActiveCategory, setView } = useStore();
  const [items, setItems] = useState<RenderItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);

  useEffect(() => {
    api.bookView()
      .then(setItems)
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, []);

  const exportMarkdown = useCallback(async () => {
    const path = await saveDialog({
      title: 'Export book as Markdown',
      defaultPath: `${book?.name ?? 'book'}.md`,
      filters: [{ name: 'Markdown', extensions: ['md'] }],
    });
    if (!path || typeof path !== 'string') return;
    setExporting(true);
    try {
      await api.exportMarkdownToFile(path);
    } catch (e) { alert(String(e)); }
    finally { setExporting(false); }
  }, [book]);

  const exportHtml = useCallback(async () => {
    const path = await saveDialog({
      title: 'Export book as HTML',
      defaultPath: `${book?.name ?? 'book'}.html`,
      filters: [{ name: 'HTML', extensions: ['html', 'htm'] }],
    });
    if (!path || typeof path !== 'string') return;
    setExporting(true);
    try {
      await api.exportHtml(path);
    } catch (e) { alert(String(e)); }
    finally { setExporting(false); }
  }, [book]);

  if (loading) return <div className="bv-state">Loading book…</div>;
  if (error) return <div className="bv-state bv-error">{error}</div>;
  if (items.length === 0) {
    return (
      <div className="bv-state bv-empty">
        <p>No sorted notes yet.</p>
        <p>Categorise notes in the Notebox to start building your book.</p>
      </div>
    );
  }

  const headings: TocEntry[] = items
    .filter((it): it is Extract<RenderItem, { kind: 'heading' }> => it.kind === 'heading')
    .map((it, i) => ({ id: headingId(it.category, i), level: it.level, text: it.text }));

  return (
    <div className="bv-root selectable">
      <div className="bv-toolbar">
        <button className="bv-export-btn" onClick={exportMarkdown} disabled={exporting}>
          Export Markdown
        </button>
        <button className="bv-export-btn" onClick={exportHtml} disabled={exporting}>
          Export HTML
        </button>
      </div>
      <div className="bv-body">
        <div className="bv-document">
          {items.map((item, i) => {
            if (item.kind === 'heading') {
              return (
                <HeadingTag
                  key={i}
                  id={headingId(item.category, i)}
                  level={item.level}
                  text={item.text}
                  onClick={item.category ? () => { setActiveCategory(item.category); setView('category'); } : undefined}
                />
              );
            }

            const note = item;
            const isListItem = note.list_depth > 0;
            const indent = isListItem ? (note.list_depth - 1) * 24 : note.indented ? 24 : 0;
            const content = note.body || note.summary;
            const accidentalWholeNoteFence = detectAccidentalWholeNoteCodeFence(content);

            return (
              <div
                key={note.id}
                className={[
                  'bv-note',
                  isListItem ? 'bv-note-list' : '',
                  note.indented ? 'bv-note-indented' : '',
                  note.join === 'same_paragraph' ? 'bv-note-inline' : '',
                ].join(' ').trim()}
                style={indent > 0 ? { marginLeft: `${indent}px` } : undefined}
                onClick={() => openEditor(note.id)}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => e.key === 'Enter' && openEditor(note.id)}
              >
                {isListItem && (
                  <span className="bv-list-marker">{note.numbered ? '1.' : '•'}</span>
                )}
                <div className="bv-note-body">
                  {accidentalWholeNoteFence && (
                    <div className="bv-repair-callout">
                      <span>This note is stored as one fenced code block.</span>
                      <button
                        type="button"
                        onClick={(event) => {
                          event.stopPropagation();
                          openEditor(note.id, 'source');
                        }}
                      >
                        Open source to repair
                      </button>
                    </div>
                  )}
                  {content.trim()
                    ? <MarkdownRenderer markdown={content} className="bv-rendered-note" />
                    : <span className="bv-empty-body">(empty)</span>}
                </div>
              </div>
            );
          })}
        </div>
        {headings.length > 0 && <BookToc headings={headings} />}
      </div>
    </div>
  );
}
