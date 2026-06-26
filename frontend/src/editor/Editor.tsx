// Unified note editor: handles all note types in one component.
// Text types use Lexical rich text or a raw markdown textarea.
// Table type uses an editable grid or a raw CSV textarea.
// The top toolbar, meta panel, LLM tools, and save logic are always shared.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { LexicalComposer, type InitialConfigType } from '@lexical/react/LexicalComposer';
import { RichTextPlugin } from '@lexical/react/LexicalRichTextPlugin';
import { ContentEditable } from '@lexical/react/LexicalContentEditable';
import { HistoryPlugin } from '@lexical/react/LexicalHistoryPlugin';
import { OnChangePlugin } from '@lexical/react/LexicalOnChangePlugin';
import { ListPlugin } from '@lexical/react/LexicalListPlugin';
import { MarkdownShortcutPlugin } from '@lexical/react/LexicalMarkdownShortcutPlugin';
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext';
import { $convertFromMarkdownString, $convertToMarkdownString, TRANSFORMERS } from '@lexical/markdown';
import type { Transformer } from '@lexical/markdown';
import { HeadingNode, QuoteNode } from '@lexical/rich-text';
import { ListNode, ListItemNode } from '@lexical/list';
import { CodeNode, CodeHighlightNode } from '@lexical/code-core';
import { LinkNode } from '@lexical/link';
import type { EditorState, LexicalEditor } from 'lexical';
import { CategoryNode } from './nodes/CategoryNode';
import { ClozeNode } from './nodes/ClozeNode';
import { PluginBlockNode } from './nodes/PluginBlockNode';
import { createPluginCodeTransformer } from './pluginCodeTransformer';
import { Toolbar } from './Toolbar';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import { Icon } from '../components/Icon';
import type { NoteDto, NoteSyncActivity } from '../types';
import { RelatedCarousel } from '../components/RelatedCarousel';
import { MetaPanel } from './MetaPanel';
import { LlmToolsMenu } from './LlmToolsMenu';
import './Editor.css';

// ── CSV helpers for table raw mode ────────────────────────────────────────────

function rowsToCsv(rows: string[][]): string {
  return rows
    .map((row) =>
      row
        .map((cell) => {
          if (cell.includes(',') || cell.includes('"') || cell.includes('\n')) {
            return `"${cell.replace(/"/g, '""')}"`;
          }
          return cell;
        })
        .join(','),
    )
    .join('\n');
}

function csvToRows(text: string): string[][] {
  const lines = text.split(/\r?\n/);
  const parsed = lines.map((line) => {
    const cells: string[] = [];
    let cell = '';
    let inQuotes = false;
    for (let i = 0; i < line.length; i++) {
      const ch = line[i];
      if (ch === '"' && inQuotes) {
        if (line[i + 1] === '"') { cell += '"'; i++; }
        else { inQuotes = false; }
      } else if (ch === '"') {
        inQuotes = true;
      } else if (ch === ',' && !inQuotes) {
        cells.push(cell); cell = '';
      } else {
        cell += ch;
      }
    }
    cells.push(cell);
    return cells;
  });
  if (parsed.length === 0 || (parsed.length === 1 && parsed[0].every((c) => !c))) {
    return defaultTableRows();
  }
  const maxCols = Math.max(...parsed.map((r) => r.length));
  return parsed.map((r) => [...r, ...Array(maxCols - r.length).fill('')]);
}

const defaultTableRows = (): string[][] => Array(5).fill(null).map(() => Array(3).fill(''));

function tsvToRows(text: string): string[][] {
  const lines = text.split(/\r?\n/).filter((l) => l.length > 0);
  if (lines.length === 0) return defaultTableRows();
  const parsed = lines.map((l) => l.split('\t'));
  const maxCols = Math.max(...parsed.map((r) => r.length));
  return parsed.map((r) => [...r, ...Array(maxCols - r.length).fill('')]);
}

function markdownTableToRows(text: string): string[][] | null {
  const lines = text.split(/\r?\n/).filter((l) => l.trim().length > 0);
  if (lines.length < 2) return null;
  // separator line looks like |---|---| or :---:|
  const isSeparator = (l: string) => /^\s*\|?[\s|:-]+\|?\s*$/.test(l) && l.includes('-');
  const tableLines = lines.filter((l) => !isSeparator(l));
  if (tableLines.length === 0) return null;
  const parsed = tableLines.map((l) => {
    const trimmed = l.trim().replace(/^\|/, '').replace(/\|$/, '');
    return trimmed.split('|').map((c) => c.trim());
  });
  const maxCols = Math.max(...parsed.map((r) => r.length));
  return parsed.map((r) => [...r, ...Array(maxCols - r.length).fill('')]);
}

function parsePastedTable(text: string): string[][] | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  const mdResult = markdownTableToRows(trimmed);
  if (mdResult) return mdResult;
  if (trimmed.includes('\t')) return tsvToRows(trimmed);
  if (trimmed.includes('\n')) return csvToRows(trimmed);
  return null;
}

// ── Lexical plugins ────────────────────────────────────────────────────────────

function InitBodyPlugin({ body, transformers }: { body: string; transformers: Transformer[] }) {
  const [editor] = useLexicalComposerContext();
  const initialised = useRef(false);
  useEffect(() => {
    if (initialised.current) return;
    initialised.current = true;
    editor.update(() => {
      $convertFromMarkdownString(body, transformers);
    }, { tag: 'init-body' });
  }, [editor, body, transformers]);
  return null;
}

function SaveShortcutPlugin({ onSave }: { onSave: () => void }) {
  const [editor] = useLexicalComposerContext();
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 's') { e.preventDefault(); onSave(); }
    };
    editor.getRootElement()?.addEventListener('keydown', handler);
    return () => editor.getRootElement()?.removeEventListener('keydown', handler);
  }, [editor, onSave]);
  return null;
}

// ── Editor ─────────────────────────────────────────────────────────────────────

interface Props { noteId: string; }

export function Editor({ noteId }: Props) {
  const { closeEditor, setCategories, categories, pluginRenderLanguages, pluginsLoaded } = useStore();

  // Map plugin-claimed code languages to a rendered PluginBlockNode; all other code fences keep
  // the built-in behavior. Used for both import (init) and export (save) so the markdown round-trips.
  const transformers = useMemo<Transformer[]>(() => {
    const pluginTransformer = createPluginCodeTransformer(pluginRenderLanguages);
    return pluginTransformer ? [pluginTransformer, ...TRANSFORMERS] : TRANSFORMERS;
  }, [pluginRenderLanguages]);
  const transformersRef = useRef(transformers);
  transformersRef.current = transformers;
  const [note, setNote] = useState<NoteDto | null>(null);
  const [title, setTitle] = useState('');
  const [summary, setSummary] = useState('');
  const [body, setBody] = useState('');
  const [rows, setRows] = useState<string[][]>([]);
  const [allNotes, setAllNotes] = useState<NoteDto[]>([]);
  const [reloadKey, setReloadKey] = useState(0);
  const [rawMode, setRawMode] = useState(false);
  const [rawText, setRawText] = useState('');
  const [dirty, setDirty] = useState(false);
  const [revision, setRevision] = useState(0);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [noteActivity, setNoteActivity] = useState<NoteSyncActivity | null>(null);
  const [imageData, setImageData] = useState<string | null>(null);

  useEffect(() => {
    setNote(null);
    setRows([]);
    setRawMode(false);
    setDirty(false);
    api.getNote(noteId)
      .then(async (n) => {
        setNote(n);
        setTitle(n.title);
        setSummary(n.summary);
        setBody(n.body);
        if (n.type === 'table') {
          const data = await api.readTableData(noteId);
          setRows(data.length > 0 ? data : defaultTableRows());
        }
      })
      .catch((e) => setError(String(e)));
  }, [noteId]);

  useEffect(() => {
    api.listNotes().then(setAllNotes).catch(() => {});
  }, [noteId]);

  useEffect(() => {
    setNoteActivity(null);
    api.noteSyncActivity(noteId).then(setNoteActivity).catch(() => {});
  }, [noteId]);

  useEffect(() => {
    // Reset stale preview immediately while the replacement asset loads.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setImageData(null);
    if (!note?.asset) return;
    api.assetData(note.asset.uuid).then(setImageData).catch((e) => setError(String(e)));
  }, [note?.asset]);

  const markDirty = useCallback(() => {
    setDirty(true);
    setRevision((r) => r + 1);
  }, []);

  const handleMetaChange = useCallback((next: NoteDto) => {
    setNote(next);
    markDirty();
  }, [markDirty]);

  // Live ref to the current markdown body (Lexical → string). Seeded from the loaded body
  // state so toggling Raw before Lexical fires OnChangePlugin returns the correct content.
  const getCurrentBody = useRef<() => string>(() => body);
  useEffect(() => { getCurrentBody.current = () => body; }, [body]);

  const handleEditorChange = useCallback((state: EditorState, _editor: LexicalEditor, tags: Set<string>) => {
    state.read(() => {
      const markdown = $convertToMarkdownString(transformersRef.current);
      getCurrentBody.current = () => markdown;
      if (!tags.has('init-body')) markDirty();
    });
  }, [markDirty]);

  // Live refs so save/autosave callbacks don't need frequent rebinding.
  const savingRef = useRef(false);
  const rawModeRef = useRef(false);
  rawModeRef.current = rawMode;
  const rawTextRef = useRef('');
  rawTextRef.current = rawText;
  const rowsRef = useRef<string[][]>([]);
  rowsRef.current = rows;
  const noteTypeRef = useRef<string>('');
  if (note) noteTypeRef.current = note.type;

  const save = useCallback(async () => {
    if (!note || savingRef.current) return;
    savingRef.current = true;
    setSaving(true);
    setError(null);
    try {
      if (noteTypeRef.current === 'table') {
        await api.saveTableData(noteId, rowsRef.current);
        const updated = await api.updateNote({ ...note, title, summary, body: '' });
        setNote(updated);
      } else {
        const updated = await api.updateNote({
          ...note,
          title,
          summary,
          body: rawModeRef.current ? rawTextRef.current : getCurrentBody.current(),
        });
        setNote(updated);
        api.allCategories().then(setCategories).catch(() => {});
      }
      setDirty(false);
    } catch (e) {
      setError(String(e));
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  }, [note, title, summary, noteId, setCategories]);

  const saveRef = useRef(save);
  saveRef.current = save;
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;

  useEffect(() => {
    if (!dirty) return;
    const timer = setTimeout(() => { void saveRef.current(); }, 1500);
    return () => clearTimeout(timer);
  }, [revision, dirty]);

  useEffect(() => {
    const flush = () => { if (dirtyRef.current) void saveRef.current(); };
    const onVisibility = () => { if (document.hidden) flush(); };
    window.addEventListener('blur', flush);
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      window.removeEventListener('blur', flush);
      document.removeEventListener('visibilitychange', onVisibility);
    };
  }, []);

  // Flush a pending save when the editor unmounts (e.g. switching to Settings or another view
  // mid-edit, which clears the autosave debounce). Without this, up to 1.5s of edits could be lost.
  useEffect(() => () => {
    if (dirtyRef.current) void saveRef.current();
    void api.noteEditingFinished(noteId);
  }, [noteId]);

  const handleBack = useCallback(async () => {
    if (dirtyRef.current) await saveRef.current();
    await api.noteEditingFinished(noteId).catch(() => {});
    closeEditor();
  }, [closeEditor, noteId]);

  const handleDelete = useCallback(async () => {
    if (!note) return;
    const isImageObject = note.type === 'picture' || note.type === 'drawing';
    const message = isImageObject
      ? 'Delete this image object and its tracked asset now? This cannot be undone.'
      : 'Move this note to trash? It will be permanently removed after the configured deletion delay.';
    if (!window.confirm(message)) return;
    try {
      if (isImageObject) {
        await api.deleteImageObjectNow(noteId);
      } else {
        await api.requestDeletion(noteId);
      }
      closeEditor();
    } catch (e) {
      setError(String(e));
    }
  }, [note, noteId, closeEditor]);

  const handleProposalApplied = useCallback((updated: NoteDto) => {
    setNote(updated);
    setTitle(updated.title);
    setSummary(updated.summary);
    setDirty(false);
    setRawMode(false);
    if (updated.type !== 'table') {
      setBody(updated.body);
      setReloadKey((k) => k + 1);
    }
    api.allCategories().then(setCategories).catch(() => {});
  }, [setCategories]);

  const toggleRaw = useCallback(() => {
    if (!rawMode) {
      const text = noteTypeRef.current === 'table'
        ? rowsToCsv(rowsRef.current)
        : getCurrentBody.current();
      setRawText(text);
      setRawMode(true);
    } else {
      if (noteTypeRef.current === 'table') {
        setRows(csvToRows(rawText));
      } else {
        setBody(rawText);
        setReloadKey((k) => k + 1);
      }
      setRawMode(false);
    }
  }, [rawMode, rawText]);

  // Table grid callbacks (only used when note.type === 'table')
  const updateCell = useCallback((r: number, c: number, value: string) => {
    setRows((prev) => { const next = prev.map((row) => [...row]); next[r][c] = value; return next; });
    markDirty();
  }, [markDirty]);

  const addRow = useCallback(() => {
    setRows((prev) => [...prev, Array(prev[0]?.length ?? 3).fill('')]);
    markDirty();
  }, [markDirty]);

  const removeRow = useCallback(() => {
    setRows((prev) => prev.length > 1 ? prev.slice(0, -1) : prev);
    markDirty();
  }, [markDirty]);

  const addCol = useCallback(() => {
    setRows((prev) => prev.map((row) => [...row, '']));
    markDirty();
  }, [markDirty]);

  const removeCol = useCallback(() => {
    setRows((prev) => (prev[0]?.length ?? 0) > 1 ? prev.map((row) => row.slice(0, -1)) : prev);
    markDirty();
  }, [markDirty]);

  const handleCellKeyDown = useCallback((
    e: React.KeyboardEvent<HTMLInputElement>, r: number, c: number,
  ) => {
    const colCount = rowsRef.current[0]?.length ?? 0;
    const rowCount = rowsRef.current.length;
    let tr = r, tc = c;
    if (e.key === 'ArrowRight' && c < colCount - 1) tc = c + 1;
    else if (e.key === 'ArrowLeft' && c > 0) tc = c - 1;
    else if (e.key === 'ArrowDown' && r < rowCount - 1) tr = r + 1;
    else if (e.key === 'ArrowUp' && r > 0) tr = r - 1;
    else if (e.key === 'Tab') {
      e.preventDefault();
      if (e.shiftKey) { if (c > 0) tc = c - 1; else if (r > 0) { tr = r - 1; tc = colCount - 1; } }
      else { if (c < colCount - 1) tc = c + 1; else if (r < rowCount - 1) { tr = r + 1; tc = 0; } }
    } else { return; }
    document.querySelector<HTMLInputElement>(`[data-cell="${tr}-${tc}"]`)?.focus();
  }, []);

  const handleTablePaste = useCallback((e: React.ClipboardEvent) => {
    const text = e.clipboardData.getData('text');
    const parsed = parsePastedTable(text);
    if (!parsed) return;
    e.preventDefault();
    setRows(parsed);
    markDirty();
  }, [markDirty]);

  // ── Render ──────────────────────────────────────────────────────────────────

  if (!note) {
    return (
      <div className="editor-loading">
        {error ? <span className="editor-error">{error}</span> : 'Loading…'}
      </div>
    );
  }

  const isTable = note.type === 'table';
  const isImageObject = note.type === 'picture' || note.type === 'drawing';
  const colCount = rows[0]?.length ?? 0;
  const rawToggleLabel = rawMode ? 'Rich text' : (isTable ? 'CSV' : 'Raw');
  const rawToggleTitle = rawMode
    ? (isTable ? 'Switch to grid view' : 'Switch to rich text')
    : (isTable ? 'Edit as raw CSV' : 'Edit raw markdown');

  const editorConfig: InitialConfigType = {
    namespace: `note-${noteId}`,
    nodes: [CategoryNode, ClozeNode, PluginBlockNode, HeadingNode, QuoteNode, ListNode, ListItemNode, CodeNode, CodeHighlightNode, LinkNode],
    onError: (err) => setError(err.message),
    theme: {
      root: 'lexical-root',
      paragraph: 'lexical-paragraph',
      heading: { h2: 'lexical-h2', h3: 'lexical-h3' },
      quote: 'lexical-quote',
      list: { ul: 'lexical-list-ul', ol: 'lexical-list-ol', listitem: 'lexical-listitem' },
      text: { bold: 'lexical-bold', italic: 'lexical-italic', underline: 'lexical-underline', code: 'lexical-code-inline', strikethrough: 'lexical-strikethrough' },
    },
  };

  return (
    <div className="editor-container selectable">
      {/* ── Top toolbar ── */}
      <div className="editor-toolbar">
        <button className="editor-back" onClick={handleBack}>
          <Icon name="arrow_back" size={16} />
          <span>Back</span>
        </button>
        <div className="editor-toolbar-center">
          <span className="editor-type-badge">{note.type}</span>
          {noteActivity && (
            <span className="editor-activity-chip" title={noteActivity.detail}>
              {activityLabel(noteActivity.kind)} {formatRelativeTime(noteActivity.happened_at)}
            </span>
          )}
        </div>
        <div className="editor-toolbar-actions">
          <LlmToolsMenu noteId={noteId} onApplied={handleProposalApplied} />
          <button className="editor-delete-btn" onClick={handleDelete} title="Delete note">
            <Icon name="delete" size={16} />
          </button>
          {dirty && <span className="editor-dirty-dot" title="Unsaved changes" />}
          <button className="editor-save-btn" onClick={save} disabled={saving || !dirty}>
            {saving ? 'Saving…' : 'Save'}
          </button>
        </div>
      </div>

      {error && <div className="editor-error-banner">{error}</div>}

      {/* ── Meta panel (shared by all types) ── */}
      <div className="editor-meta">
        <input
          className="editor-title"
          value={title}
          onChange={(e) => { setTitle(e.target.value); markDirty(); }}
          placeholder="Note title…"
        />
        <input
          className="editor-summary"
          value={summary}
          onChange={(e) => { setSummary(e.target.value); markDirty(); }}
          placeholder="One-line summary (optional)…"
        />
        <div className="editor-categories">
          {note.categories.map((c) => (
            <span key={c} className="editor-category-chip">#{c}</span>
          ))}
        </div>
        <MetaPanel note={note} categories={categories} allNotes={allNotes} onChange={handleMetaChange} />
      </div>

      {/* ── Body / Data area ── */}
      <div className="editor-body-header">
        <span className="editor-body-label">{isTable ? 'Data' : (isImageObject ? 'Description' : 'Body')}</span>
        {!isImageObject && (
          <button className="editor-raw-toggle" onClick={toggleRaw} title={rawToggleTitle}>
            {rawToggleLabel}
          </button>
        )}
      </div>

      {isImageObject ? (
        <div className="editor-image-object">
          <div className="editor-image-preview">
            {imageData ? (
              <img src={imageData} alt={title || note.asset?.original_filename || 'Imported image'} />
            ) : (
              <div className="editor-image-missing">Image asset is missing.</div>
            )}
          </div>
          {note.asset && (
            <div className="editor-image-facts">
              {note.asset.media_type} · {note.asset.intrinsic_dimensions[0]} × {note.asset.intrinsic_dimensions[1]} · {note.asset.original_filename}
            </div>
          )}
          <textarea
            className="editor-image-description"
            value={body}
            onChange={(event) => {
              const value = event.target.value;
              setBody(value);
              getCurrentBody.current = () => value;
              markDirty();
            }}
            placeholder="Caption, provenance, or description…"
          />
        </div>
      ) : isTable && !rawMode ? (
        // Table: spreadsheet grid
        <div className="table-editor-area">
          <div className="table-editor-controls">
            <button className="table-ctrl-btn" onClick={addRow}>+ Row</button>
            <button className="table-ctrl-btn" onClick={removeRow} disabled={rows.length <= 1}>− Row</button>
            <div className="table-ctrl-divider" />
            <button className="table-ctrl-btn" onClick={addCol}>+ Col</button>
            <button className="table-ctrl-btn" onClick={removeCol} disabled={colCount <= 1}>− Col</button>
          </div>
          <div className="table-editor-scroll" onPaste={handleTablePaste}>
            <table className="table-grid">
              <tbody>
                {rows.map((row, r) => (
                  <tr key={r} className={r === 0 ? 'table-header-row' : ''}>
                    {row.map((cell, c) => (
                      <td key={c} className="table-grid-cell">
                        <input
                          className="table-cell-input"
                          value={cell}
                          data-cell={`${r}-${c}`}
                          onChange={(e) => updateCell(r, c, e.target.value)}
                          onKeyDown={(e) => handleCellKeyDown(e, r, c)}
                        />
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      ) : rawMode ? (
        // Raw textarea (markdown for text notes, CSV for table notes)
        <div className="editor-body-wrap">
          <textarea
            className="editor-raw-textarea"
            value={rawText}
            onChange={(e) => { setRawText(e.target.value); markDirty(); }}
            spellCheck={false}
          />
        </div>
      ) : !pluginsLoaded ? (
        // Hold until list_plugins() settles so InitBodyPlugin fires with the correct transformers.
        <div className="editor-body-wrap"><div className="editor-loading">Loading…</div></div>
      ) : (
        // Text note: Lexical rich text editor
        <LexicalComposer key={`${noteId}-${reloadKey}`} initialConfig={editorConfig}>
          <Toolbar />
          <div className="editor-body-wrap">
            <RichTextPlugin
              contentEditable={<ContentEditable className="lexical-content-editable" />}
              placeholder={<div className="lexical-placeholder">Write the note body here…</div>}
              ErrorBoundary={({ onError, children }) => {
                try { return <>{children}</>; }
                catch (e) { onError(e as Error); return null; }
              }}
            />
          </div>
          <HistoryPlugin />
          <ListPlugin />
          <MarkdownShortcutPlugin transformers={transformers} />
          <OnChangePlugin onChange={handleEditorChange} />
          <InitBodyPlugin body={body} transformers={transformers} />
          <SaveShortcutPlugin onSave={save} />
        </LexicalComposer>
      )}

      <RelatedCarousel noteId={noteId} />
    </div>
  );
}

function activityLabel(kind: string) {
  if (kind === 'external_update') return 'External update';
  if (kind === 'remote_loro_merge') return 'Remote Loro merge';
  if (kind === 'conflict_detected') return 'Conflict copy';
  return kind.replaceAll('_', ' ');
}

function formatRelativeTime(value: string) {
  const timestamp = new Date(value).getTime();
  if (Number.isNaN(timestamp)) return value;
  const seconds = Math.max(0, Math.round((Date.now() - timestamp) / 1000));
  if (seconds < 60) return 'just now';
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 48) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}
