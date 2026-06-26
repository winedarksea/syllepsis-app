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
import { $getSelection, $isRangeSelection } from 'lexical';
import type { EditorState, LexicalEditor } from 'lexical';
import { CategoryNode } from './nodes/CategoryNode';
import { ClozeNode } from './nodes/ClozeNode';
import { PluginBlockNode } from './nodes/PluginBlockNode';
import { createPluginCodeTransformer } from './pluginCodeTransformer';
import { Toolbar } from './Toolbar';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import { Icon } from '../components/Icon';
import { MarkdownRenderer } from '../components/MarkdownRenderer';
import { detectAccidentalWholeNoteCodeFence } from '../lib/wholeNoteFence';
import type { Category, LookupEntry, NoteDto, NoteEmbeddingDetails, NoteNeighbors, NoteScreenMode, NoteSyncActivity } from '../types';
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

interface CompletionItem {
  label: string;
  insert: string;
}

function AutocompletePlugin({
  categories,
  notes,
}: {
  categories: Category[];
  notes: NoteDto[];
}) {
  const [editor] = useLexicalComposerContext();
  const [token, setToken] = useState('');
  const [locations, setLocations] = useState<LookupEntry[]>([]);

  useEffect(() => {
    api.locationLookup().then(setLocations).catch(() => setLocations([]));
  }, []);

  useEffect(() => editor.registerUpdateListener(({ editorState }) => {
    editorState.read(() => {
      const selection = $getSelection();
      if (!$isRangeSelection(selection) || !selection.isCollapsed()) {
        setToken('');
        return;
      }
      const anchor = selection.anchor;
      const text = anchor.getNode().getTextContent().slice(0, anchor.offset);
      const match = text.match(/(#([\w-]*)|@([\w-]*)|(?:due|start|done|loc|waiting|blocked-by):([\w-]*))$/);
      setToken(match?.[0] ?? '');
    });
  }), [editor]);

  const completions = useMemo<CompletionItem[]>(() => {
    if (!token) return [];
    if (token.startsWith('#')) {
      const prefix = token.slice(1).toLowerCase();
      return categories
        .filter((category) => category.name.toLowerCase().startsWith(prefix))
        .slice(0, 6)
        .map((category) => ({ label: `#${category.name}`, insert: `#${category.name}` }));
    }
    if (token.startsWith('@')) {
      const prefix = token.slice(1).toLowerCase();
      return notes
        .filter((note) => note.id.toLowerCase().includes(prefix) || note.title.toLowerCase().includes(prefix))
        .slice(0, 6)
        .map((note) => ({ label: `${note.title || note.id}`, insert: `@${note.id}` }));
    }
    const [kind, value = ''] = token.split(':');
    const prefix = value.toLowerCase();
    if (kind === 'loc') {
      return locations
        .filter((location) => location.name.toLowerCase().startsWith(prefix))
        .slice(0, 6)
        .map((location) => ({ label: `loc:${location.name}`, insert: `loc:${location.name}` }));
    }
    if (kind === 'waiting' || kind === 'blocked-by') {
      return notes
        .filter((note) => note.type === 'todo')
        .filter((note) => note.id.toLowerCase().includes(prefix) || note.title.toLowerCase().includes(prefix))
        .slice(0, 6)
        .map((note) => ({ label: `${kind}:${note.title || note.id}`, insert: `${kind}:${note.id}` }));
    }
    if (kind === 'due' || kind === 'start' || kind === 'done') {
      const today = new Date();
      const tomorrow = new Date(today.getTime() + 86_400_000);
      const fmt = (date: Date) => date.toISOString().slice(0, 10);
      return [
        { label: `${kind}:${fmt(today)}`, insert: `${kind}:${fmt(today)}` },
        { label: `${kind}:${fmt(tomorrow)}`, insert: `${kind}:${fmt(tomorrow)}` },
      ];
    }
    return [];
  }, [categories, locations, notes, token]);

  const applyCompletion = useCallback((completion: CompletionItem) => {
    editor.update(() => {
      const selection = $getSelection();
      if (!$isRangeSelection(selection)) return;
      selection.insertText(completion.insert.slice(token.length));
    });
    setToken('');
  }, [editor, token]);

  if (completions.length === 0) return null;
  return (
    <div className="editor-autocomplete-menu">
      {completions.map((completion) => (
        <button
          key={completion.insert}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => applyCompletion(completion)}
        >
          {completion.label}
        </button>
      ))}
    </div>
  );
}

// ── Editor ─────────────────────────────────────────────────────────────────────

interface Props {
  noteId: string;
  initialMode?: NoteScreenMode;
}

export function Editor({ noteId, initialMode = 'read' }: Props) {
  const { closeEditor, openEditor, setCategories, setActiveCategory, setView, categories, pluginRenderLanguages, pluginsLoaded, noteReloadSignal } = useStore();

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
  const [mode, setMode] = useState<NoteScreenMode>(initialMode);
  const [rawText, setRawText] = useState('');
  const rawTextareaRef = useRef<HTMLTextAreaElement | null>(null);
  const [dirty, setDirty] = useState(false);
  const [revision, setRevision] = useState(0);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [noteActivity, setNoteActivity] = useState<NoteSyncActivity | null>(null);
  const [imageData, setImageData] = useState<string | null>(null);
  const [neighbors, setNeighbors] = useState<NoteNeighbors>({});
  const [charCount, setCharCount] = useState<number | null>(null);
  const [embeddingDetails, setEmbeddingDetails] = useState<NoteEmbeddingDetails | null>(null);
  const [findOpen, setFindOpen] = useState(false);
  const [findPattern, setFindPattern] = useState('');
  const [findIndex, setFindIndex] = useState(0);
  const [readModeMatchCount, setReadModeMatchCount] = useState(0);
  const [mergeDialogOpen, setMergeDialogOpen] = useState(false);
  const [splitDialogOpen, setSplitDialogOpen] = useState(false);

  useEffect(() => {
    setNote(null);
    setRows([]);
    setMode(initialMode);
    setRawText('');
    setDirty(false);
    api.getNote(noteId)
      .then(async (n) => {
        setNote(n);
        setTitle(n.title);
        setSummary(n.summary);
        setBody(n.body);
        if (initialMode === 'source') setRawText(n.body);
        if (n.type === 'table') {
          const data = await api.readTableData(noteId);
          setRows(data.length > 0 ? data : defaultTableRows());
        }
      })
      .catch((e) => setError(String(e)));
  }, [initialMode, noteId, noteReloadSignal]);

  useEffect(() => {
    api.listNotes().then(setAllNotes).catch(() => {});
  }, [noteId]);

  useEffect(() => {
    api.noteNeighbors(noteId).then(setNeighbors).catch(() => setNeighbors({}));
    api.noteEmbeddingDetails(noteId).then(setEmbeddingDetails).catch(() => setEmbeddingDetails(null));
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
  const modeRef = useRef<NoteScreenMode>('read');
  modeRef.current = mode;
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
          body: modeRef.current === 'source' ? rawTextRef.current : getCurrentBody.current(),
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

  const switchMode = useCallback((nextMode: NoteScreenMode) => {
    if (nextMode === mode) return;
    if (mode === 'source') {
      if (noteTypeRef.current === 'table') {
        setRows(csvToRows(rawTextRef.current));
      } else {
        const text = rawTextRef.current;
        setBody(text);
        getCurrentBody.current = () => text;
        setReloadKey((k) => k + 1);
      }
    } else if (nextMode === 'source') {
      const text = noteTypeRef.current === 'table'
        ? rowsToCsv(rowsRef.current)
        : getCurrentBody.current();
      setRawText(text);
    } else if (mode === 'edit') {
      const text = getCurrentBody.current();
      setBody(text);
    }
    setMode(nextMode);
  }, [mode]);

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

  const unwrapWholeNoteFence = useCallback(() => {
    const detected = detectAccidentalWholeNoteCodeFence(
      modeRef.current === 'source' ? rawTextRef.current : body,
    );
    if (!detected) return;
    setBody(detected.innerMarkdown);
    setRawText(detected.innerMarkdown);
    getCurrentBody.current = () => detected.innerMarkdown;
    markDirty();
  }, [body, markDirty]);

  const bodyForRead = useMemo(
    () => mode === 'source' ? rawText : mode === 'read' ? body : getCurrentBody.current(),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [mode, rawText, revision, body],
  );

  const accidentalWholeNoteFence = useMemo(
    () => (mode === 'read' || mode === 'source')
      ? detectAccidentalWholeNoteCodeFence(bodyForRead)
      : null,
    [bodyForRead, mode],
  );

  useEffect(() => {
    if (mode === 'read' || isImageObjectType(noteTypeRef.current)) {
      setCharCount(null);
      return;
    }
    const text = mode === 'source' ? rawText : getCurrentBody.current();
    setCharCount(text.length);
  }, [mode, rawText, revision]);

  // Read mode: count comes from MarkdownRenderer via onMatchCount callback (DOM-accurate).
  // Other modes: count against the raw text.
  const findMatchCount = useMemo(() => {
    if (!findPattern.trim()) return 0;
    if (mode === 'read') return readModeMatchCount;
    try {
      return [...bodyForRead.matchAll(new RegExp(findPattern, 'g'))].length;
    } catch {
      return 0;
    }
  }, [bodyForRead, findPattern, mode, readModeMatchCount]);

  const findError = useMemo(() => {
    if (!findPattern.trim()) return null;
    try {
      new RegExp(findPattern, 'g');
      return null;
    } catch (error) {
      return error instanceof Error ? error.message : String(error);
    }
  }, [findPattern]);

  const moveFind = useCallback((direction: 1 | -1) => {
    if (findMatchCount === 0) return;
    setFindIndex((current) => {
      const next = (current + direction + findMatchCount) % findMatchCount;
      if (mode === 'source') {
        const regex = new RegExp(findPattern, 'g');
        let index = 0;
        let match: RegExpExecArray | null;
        while ((match = regex.exec(rawTextRef.current)) !== null) {
          if (index === next) {
            requestAnimationFrame(() => {
              const el = rawTextareaRef.current;
              if (!el) return;
              el.focus();
              el.setSelectionRange(match!.index, match!.index + match![0].length);
              // Scroll so the selection is visible
              const lineHeight = parseInt(getComputedStyle(el).lineHeight || '20', 10);
              const linesBefore = rawTextRef.current.slice(0, match!.index).split('\n').length - 1;
              el.scrollTop = Math.max(0, linesBefore * lineHeight - el.clientHeight / 2);
            });
            break;
          }
          index += 1;
          if (match[0].length === 0) regex.lastIndex += 1;
        }
      }
      return next;
    });
  }, [findMatchCount, findPattern, mode]);

  useEffect(() => {
    setFindIndex(0);
  }, [findPattern, noteId]);

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'f') {
        event.preventDefault();
        setFindOpen(true);
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, []);

  const handleMerge = useCallback(async (sourceIds: string[]) => {
    if (!note) return;
    if (sourceIds.length === 0) return;
    if (dirtyRef.current) await saveRef.current();
    try {
      const updated = await api.mergeNotes({
        target_note_id: note.id,
        source_note_ids: sourceIds,
      });
      setNote(updated);
      setTitle(updated.title);
      setSummary(updated.summary);
      setBody(updated.body);
      getCurrentBody.current = () => updated.body;
      setReloadKey((key) => key + 1);
      setDirty(false);
      setMode('read');
      api.allCategories().then(setCategories).catch(() => {});
    } catch (error) {
      setError(String(error));
    }
  }, [note, setCategories]);

  const handleSplit = useCallback(async (splitAt: number, secondTitle?: string) => {
    if (!note || note.type === 'table' || note.type === 'picture' || note.type === 'drawing') return;
    if (!Number.isFinite(splitAt) || splitAt < 0) {
      setError('Split offset must be a non-negative number.');
      return;
    }
    if (dirtyRef.current) await saveRef.current();
    try {
      const result = await api.splitNote({ note_id: note.id, split_at: splitAt, second_title: secondTitle || null });
      setNote(result.first);
      setTitle(result.first.title);
      setSummary(result.first.summary);
      setBody(result.first.body);
      getCurrentBody.current = () => result.first.body;
      setReloadKey((key) => key + 1);
      setDirty(false);
      setMode('read');
      api.noteNeighbors(note.id).then(setNeighbors).catch(() => {});
    } catch (error) {
      setError(String(error));
    }
  }, [note]);

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
        <button
          className="editor-nav-btn"
          disabled={!neighbors.previous}
          title={neighbors.previous ? `Previous: ${displayNeighbor(neighbors.previous)}` : 'No previous sorted note'}
          onClick={() => neighbors.previous && openEditor(neighbors.previous.id, 'read')}
        >
          <Icon name="chevron_left" size={18} />
        </button>
        <button
          className="editor-nav-btn"
          disabled={!neighbors.next}
          title={neighbors.next ? `Next: ${displayNeighbor(neighbors.next)}` : 'No next sorted note'}
          onClick={() => neighbors.next && openEditor(neighbors.next.id, 'read')}
        >
          <Icon name="chevron_right" size={18} />
        </button>
        <div className="editor-toolbar-center">
          <NoteModeSwitcher mode={mode} disabled={isImageObject} onChange={switchMode} />
          <span className="editor-type-badge">{note.type}</span>
          {noteActivity && (
            <span className="editor-activity-chip" title={noteActivity.detail}>
              {activityLabel(noteActivity.kind)} {formatRelativeTime(noteActivity.happened_at)}
            </span>
          )}
        </div>
        <div className="editor-toolbar-actions">
          <button className="editor-tool-btn" onClick={() => setFindOpen((open) => !open)} title="Find in note">
            <Icon name="search" size={16} />
          </button>
          <button className="editor-tool-btn" onClick={() => setMergeDialogOpen(true)} title="Merge another note into this note">
            Merge
          </button>
          <button
            className="editor-tool-btn"
            onClick={() => setSplitDialogOpen(true)}
            title="Split this note at an offset"
            disabled={isTable || isImageObject}
          >
            Split
          </button>
          <LlmToolsMenu noteId={noteId} />
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
      {findOpen && (
        <FindBar
          pattern={findPattern}
          matchCount={findMatchCount}
          matchIndex={findIndex}
          error={findError}
          onPatternChange={setFindPattern}
          onNext={() => moveFind(1)}
          onPrevious={() => moveFind(-1)}
          onClose={() => setFindOpen(false)}
        />
      )}

      {/* ── Meta panel (shared by all types) ── */}
      <div className="editor-meta">
        <input
          className="editor-title"
          value={title}
          readOnly={mode === 'read'}
          onChange={(e) => { setTitle(e.target.value); markDirty(); }}
          placeholder="Note title…"
        />
        <input
          className="editor-summary"
          value={summary}
          readOnly={mode === 'read'}
          onChange={(e) => { setSummary(e.target.value); markDirty(); }}
          placeholder="One-line summary (optional)…"
        />
        <div className="editor-categories">
          {note.categories.map((c) => (
            <button
              key={c}
              className="editor-category-chip"
              onClick={() => {
                setActiveCategory(c);
                setView('category');
              }}
            >
              #{c}
            </button>
          ))}
        </div>
        <MetaPanel note={note} categories={categories} allNotes={allNotes} embeddingDetails={embeddingDetails} onChange={handleMetaChange} />
      </div>

      {/* ── Body / Data area ── */}
      <div className="editor-body-header">
        <span className="editor-body-label">{isTable ? 'Data' : (isImageObject ? 'Description' : 'Body')}</span>
        <BodyStats count={charCount} visible={mode !== 'read' && !isImageObject} />
      </div>

      {accidentalWholeNoteFence && !isTable && !isImageObject && (
        <div className="editor-repair-banner">
          <span>This note is stored as one fenced code block, so its markdown is shown as code.</span>
          <button type="button" onClick={unwrapWholeNoteFence}>
            Unwrap outer fence
          </button>
        </div>
      )}

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
      ) : mode === 'read' ? (
        <div className="editor-read-wrap">
          <MarkdownRenderer
            markdown={bodyForRead}
            className="editor-read-body"
            findPattern={findError ? '' : findPattern}
            findMatchIndex={findIndex}
            onMatchCount={setReadModeMatchCount}
          />
        </div>
      ) : isTable && mode !== 'source' ? (
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
      ) : mode === 'source' ? (
        // Raw textarea (markdown for text notes, CSV for table notes)
        <div className="editor-body-wrap">
          <textarea
            ref={rawTextareaRef}
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
          <AutocompletePlugin categories={categories} notes={allNotes} />
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
      {mergeDialogOpen && (
        <MergeDialog
          target={note}
          allNotes={allNotes}
          onClose={() => setMergeDialogOpen(false)}
          onMerge={(sourceIds) => {
            setMergeDialogOpen(false);
            void handleMerge(sourceIds);
          }}
        />
      )}
      {splitDialogOpen && (
        <SplitDialog
          body={getCurrentBody.current()}
          defaultOffset={mode === 'source' ? rawTextareaRef.current?.selectionStart ?? rawText.length : getCurrentBody.current().length}
          onClose={() => setSplitDialogOpen(false)}
          onSplit={(splitAt, secondTitle) => {
            setSplitDialogOpen(false);
            void handleSplit(splitAt, secondTitle);
          }}
        />
      )}
    </div>
  );
}

function activityLabel(kind: string) {
  if (kind === 'external_update') return 'External update';
  if (kind === 'remote_loro_merge') return 'Remote Loro merge';
  if (kind === 'conflict_detected') return 'Conflict copy';
  return kind.replaceAll('_', ' ');
}

function isImageObjectType(type: string) {
  return type === 'picture' || type === 'drawing';
}

function displayNeighbor(neighbor: { title: string; summary: string }) {
  return neighbor.title || neighbor.summary || '(untitled)';
}

function NoteModeSwitcher({
  mode,
  disabled,
  onChange,
}: {
  mode: NoteScreenMode;
  disabled: boolean;
  onChange: (mode: NoteScreenMode) => void;
}) {
  if (disabled) return null;
  const modes: NoteScreenMode[] = ['read', 'edit', 'source'];
  return (
    <div className="editor-mode-switcher" role="tablist" aria-label="Note mode">
      {modes.map((candidate) => (
        <button
          key={candidate}
          className={candidate === mode ? 'active' : ''}
          onClick={() => onChange(candidate)}
          aria-pressed={candidate === mode}
        >
          {candidate}
        </button>
      ))}
    </div>
  );
}

function FindBar({
  pattern,
  matchCount,
  matchIndex,
  error,
  onPatternChange,
  onNext,
  onPrevious,
  onClose,
}: {
  pattern: string;
  matchCount: number;
  matchIndex: number;
  error: string | null;
  onPatternChange: (pattern: string) => void;
  onNext: () => void;
  onPrevious: () => void;
  onClose: () => void;
}) {
  return (
    <div className="editor-find-bar">
      <input
        autoFocus
        value={pattern}
        onChange={(event) => onPatternChange(event.target.value)}
        placeholder="Regex find"
      />
      <span className={error ? 'editor-find-error' : 'editor-find-count'}>
        {error ? 'Invalid regex' : matchCount > 0 ? `${matchIndex + 1}/${matchCount}` : '0/0'}
      </span>
      <button onClick={onPrevious} disabled={!!error || matchCount === 0} title="Previous match">
        <Icon name="keyboard_arrow_up" size={16} />
      </button>
      <button onClick={onNext} disabled={!!error || matchCount === 0} title="Next match">
        <Icon name="keyboard_arrow_down" size={16} />
      </button>
      <button onClick={onClose} title="Close find">
        <Icon name="close" size={16} />
      </button>
    </div>
  );
}

function MergeDialog({
  target,
  allNotes,
  onClose,
  onMerge,
}: {
  target: NoteDto;
  allNotes: NoteDto[];
  onClose: () => void;
  onMerge: (sourceIds: string[]) => void;
}) {
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<string[]>([]);
  const candidates = useMemo(() => {
    const q = query.trim().toLowerCase();
    return allNotes
      .filter((note) => note.id !== target.id)
      .filter((note) => !q || note.id.toLowerCase().includes(q) || note.title.toLowerCase().includes(q) || note.summary.toLowerCase().includes(q))
      .slice(0, 40);
  }, [allNotes, query, target.id]);

  const toggle = (id: string) => {
    setSelected((current) => current.includes(id)
      ? current.filter((candidate) => candidate !== id)
      : [...current, id]);
  };

  return (
    <div className="editor-dialog-backdrop" role="presentation" onClick={onClose}>
      <div className="editor-dialog" role="dialog" aria-modal="true" onClick={(event) => event.stopPropagation()}>
        <div className="editor-dialog-header">
          <h3>Merge into current note</h3>
          <button onClick={onClose} aria-label="Close"><Icon name="close" size={16} /></button>
        </div>
        <div className="editor-dialog-body">
          <div className="editor-dialog-target">
            Target: <strong>{target.title || target.id}</strong>
          </div>
          <input
            className="editor-dialog-search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search notes to merge..."
            autoFocus
          />
          <div className="editor-merge-list">
            {candidates.map((note) => (
              <label key={note.id} className="editor-merge-row">
                <input
                  type="checkbox"
                  checked={selected.includes(note.id)}
                  onChange={() => toggle(note.id)}
                />
                <span className="editor-merge-copy">
                  <strong>{note.title || note.id}</strong>
                  <small>{note.summary || note.body.slice(0, 120)}</small>
                </span>
              </label>
            ))}
          </div>
        </div>
        <div className="editor-dialog-actions">
          <button className="picker-btn picker-btn-secondary" onClick={onClose}>Cancel</button>
          <button className="picker-btn picker-btn-primary" disabled={selected.length === 0} onClick={() => onMerge(selected)}>
            Merge {selected.length || ''}
          </button>
        </div>
      </div>
    </div>
  );
}

function SplitDialog({
  body,
  defaultOffset,
  onClose,
  onSplit,
}: {
  body: string;
  defaultOffset: number;
  onClose: () => void;
  onSplit: (splitAt: number, secondTitle?: string) => void;
}) {
  const [offset, setOffset] = useState(String(defaultOffset));
  const [secondTitle, setSecondTitle] = useState('');
  const splitAt = Math.max(0, Math.min(body.length, Number.parseInt(offset, 10) || 0));
  const first = body.slice(0, splitAt).trimEnd();
  const second = body.slice(splitAt).trimStart();

  return (
    <div className="editor-dialog-backdrop" role="presentation" onClick={onClose}>
      <div className="editor-dialog editor-split-dialog" role="dialog" aria-modal="true" onClick={(event) => event.stopPropagation()}>
        <div className="editor-dialog-header">
          <h3>Split note</h3>
          <button onClick={onClose} aria-label="Close"><Icon name="close" size={16} /></button>
        </div>
        <div className="editor-dialog-body">
          <label className="editor-dialog-field">
            <span>Character offset</span>
            <input value={offset} onChange={(event) => setOffset(event.target.value)} inputMode="numeric" autoFocus />
          </label>
          <label className="editor-dialog-field">
            <span>Second note title</span>
            <input value={secondTitle} onChange={(event) => setSecondTitle(event.target.value)} placeholder="Optional" />
          </label>
          <div className="editor-split-preview">
            <div>
              <strong>First note</strong>
              <pre>{first || '(empty)'}</pre>
            </div>
            <div>
              <strong>Second note</strong>
              <pre>{second || '(empty)'}</pre>
            </div>
          </div>
        </div>
        <div className="editor-dialog-actions">
          <button className="picker-btn picker-btn-secondary" onClick={onClose}>Cancel</button>
          <button className="picker-btn picker-btn-primary" disabled={!second.trim()} onClick={() => onSplit(splitAt, secondTitle.trim() || undefined)}>
            Split note
          </button>
        </div>
      </div>
    </div>
  );
}

function BodyStats({ count, visible }: { count: number | null; visible: boolean }) {
  if (!visible || count === null) return null;
  const cls = count > 8000 ? ' warning' : count > 6500 ? ' caution' : '';
  return (
    <span className={`editor-body-stats${cls}`}>
      {count.toLocaleString()} chars
    </span>
  );
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
