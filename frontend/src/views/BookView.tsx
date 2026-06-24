// Renders the sorted book as a continuous document (headings + paragraphs/bullets).
// Export buttons let users save the book as Markdown or HTML.

import { useCallback, useEffect, useMemo, useState } from 'react';
import { save as saveDialog } from '@tauri-apps/plugin-dialog';
import { api } from '../lib/api';
import { sanitizeHtml } from '../lib/sanitize';
import { useStore } from '../lib/store';
import type { RenderItem } from '../types';
import './BookView.css';

// ── Body segment parser ──────────────────────────────────────────────────────

type TextSegment = { kind: 'text'; content: string };
type CodeSegment = { kind: 'code'; language: string; code: string };
type BodySegment = TextSegment | CodeSegment;

function parseBodySegments(body: string): BodySegment[] {
  const segments: BodySegment[] = [];
  // Match fenced code blocks on their own lines; back-reference [1] for the fence length.
  const fenceRe = /^(`{3,})(\w*)[ \t]*\r?\n([\s\S]*?)^\1[ \t]*$/gm;
  let lastIndex = 0;
  let match;
  while ((match = fenceRe.exec(body)) !== null) {
    if (match.index > lastIndex) {
      segments.push({ kind: 'text', content: body.slice(lastIndex, match.index) });
    }
    segments.push({ kind: 'code', language: match[2] || '', code: match[3] });
    lastIndex = match.index + match[0].length;
  }
  if (lastIndex < body.length) {
    segments.push({ kind: 'text', content: body.slice(lastIndex) });
  }
  return segments.length > 0 ? segments : [{ kind: 'text', content: body }];
}

// ── BookCodeBlock ────────────────────────────────────────────────────────────

function BookCodeBlock({
  language,
  code,
  claimed,
}: {
  language: string;
  code: string;
  claimed: Set<string>;
}) {
  const [html, setHtml] = useState<string | null>(null);
  const isClaimed = language.length > 0 && claimed.has(language.toLowerCase());

  useEffect(() => {
    if (!isClaimed) return;
    let active = true;
    setHtml(null);
    api
      .runRenderPlugin(language, code)
      .then((raw) => { if (active) setHtml(sanitizeHtml(raw)); })
      .catch(() => { if (active) setHtml(null); });
    return () => { active = false; };
  }, [language, code, isClaimed]);

  if (isClaimed && html !== null) {
    return (
      <div
        className="bv-plugin-block"
        data-language={language}
        dangerouslySetInnerHTML={{ __html: html }}
      />
    );
  }
  return (
    <pre className="bv-code-block" data-language={language || undefined}>
      <code>{code}</code>
    </pre>
  );
}

// ── BookNoteBody ─────────────────────────────────────────────────────────────

function BookNoteBody({ body, claimed }: { body: string; claimed: Set<string> }) {
  const segments = useMemo(() => parseBodySegments(body), [body]);
  const hasCode = segments.some((s) => s.kind === 'code');

  if (!hasCode) return <>{body.trim()}</>;

  return (
    <>
      {segments.map((seg, i) =>
        seg.kind === 'code' ? (
          <BookCodeBlock key={i} language={seg.language} code={seg.code} claimed={claimed} />
        ) : (
          <span key={i}>{seg.content}</span>
        )
      )}
    </>
  );
}

// ── HeadingTag ───────────────────────────────────────────────────────────────

function HeadingTag({ level, text }: { level: number; text: string }) {
  const l = Math.min(Math.max(level, 1), 6);
  const cls = `bv-heading bv-h${l}`;
  if (l === 1) return <h1 className={cls}>{text}</h1>;
  if (l === 2) return <h2 className={cls}>{text}</h2>;
  if (l === 3) return <h3 className={cls}>{text}</h3>;
  if (l === 4) return <h4 className={cls}>{text}</h4>;
  if (l === 5) return <h5 className={cls}>{text}</h5>;
  return <h6 className={cls}>{text}</h6>;
}

// ── BookView ─────────────────────────────────────────────────────────────────

export function BookView() {
  const { openEditor, book, pluginRenderLanguages } = useStore();
  const claimed = useMemo(() => new Set(pluginRenderLanguages), [pluginRenderLanguages]);
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
        <p>Categorise notes in the Unsorted queue to start building your book.</p>
      </div>
    );
  }

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
      <div className="bv-document">
        {items.map((item, i) => {
          if (item.kind === 'heading') {
            return <HeadingTag key={i} level={item.level} text={item.text} />;
          }

          const note = item;
          const isListItem = note.list_depth > 0;
          const indent = isListItem ? (note.list_depth - 1) * 24 : note.indented ? 24 : 0;
          const content = note.body || note.summary;

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
                {content.trim()
                  ? <BookNoteBody body={content} claimed={claimed} />
                  : <span className="bv-empty-body">(empty)</span>}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
