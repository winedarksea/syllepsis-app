// Excalidraw-based drawing editor for Drawing-type notes.
// The SVG asset carries the scene in <metadata> (exportEmbedScene: true).
// Notes without an embedded scene open view-only (externally imported SVGs).

import '@excalidraw/excalidraw/index.css';
import { Component, useCallback, useEffect, useRef, useState } from 'react';
import type { ErrorInfo, ReactNode } from 'react';
import { Excalidraw, exportToSvg, serializeAsJSON, hashElementsVersion } from '@excalidraw/excalidraw';
import type {
  ExcalidrawImperativeAPI,
  ExcalidrawInitialDataState,
  AppState,
  BinaryFiles,
} from '@excalidraw/excalidraw/types';
import type { NonDeletedExcalidrawElement, ExcalidrawElement } from '@excalidraw/excalidraw/element/types';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import type { NoteDto } from '../types';

interface Props {
  note: NoteDto;
  markDirty: () => void;
  /** Ref populated with a function that produces the current SVG for the save path. */
  getSvgRef: React.MutableRefObject<(() => Promise<string | null>) | null>;
  /** Called with the latest note after a successful SVG save (body links may be refreshed). */
  onSaved?: (updated: NoteDto) => void;
}

type WithLink = { link?: string | null };

/** Decode the XML entities that XMLSerializer escapes inside textContent so the
 *  recovered string is valid JSON again. */
function decodeXmlEntities(text: string): string {
  return text
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&apos;/g, "'")
    .replace(/&amp;/g, '&');
}

/** Extract the Excalidraw JSON scene from inside an SVG's <metadata> block.
 *  The scene may be embedded bare, preceded by a comment marker
 *  (`<!-- payload-type:... -->{json}`, our create format), or wrapped inside a
 *  comment (`<!-- {json} -->`, Excalidraw's own UI export). We try the metadata
 *  content with all comments stripped, then each comment's inner text, and use
 *  the first candidate that looks like an Excalidraw scene. */
function extractSceneFromSvg(svg: string): string | null {
  const metaMatch = svg.match(/<metadata[^>]*>([\s\S]*?)<\/metadata>/i);
  if (!metaMatch) return null;
  const content = metaMatch[1];
  const candidates: string[] = [content.replace(/<!--[\s\S]*?-->/g, '').trim()];
  for (const m of content.matchAll(/<!--([\s\S]*?)-->/g)) {
    candidates.push(m[1].trim());
  }
  // Whitespace-tolerant: serializeAsJSON pretty-prints (`"type": "excalidraw"`), while the
  // backend's blank scene is compact (`"type":"excalidraw"`).
  const sceneMarker = /"type"\s*:\s*"excalidraw"/;
  for (const candidate of candidates) {
    if (sceneMarker.test(candidate)) {
      return decodeXmlEntities(candidate);
    }
  }
  return null;
}

/** Return `syllepsis://note/<id>` links present in an element list. */
function noteLinksFromElements(elements: readonly ExcalidrawElement[]): Set<string> {
  const ids = new Set<string>();
  for (const el of elements) {
    const link = (el as ExcalidrawElement & WithLink).link;
    if (link?.startsWith('syllepsis://note/')) {
      const id = link.slice('syllepsis://note/'.length);
      if (id) ids.add(id);
    }
  }
  return ids;
}

/** Parse linked-note IDs from a markdown body. */
function parseLinkedNoteIds(body: string): Set<string> {
  const ids = new Set<string>();
  const re = /\(syllepsis:\/\/note\/([A-Za-z0-9_-]+)\)/g;
  let m: RegExpExecArray | null;
  // eslint-disable-next-line no-cond-assign
  while ((m = re.exec(body)) !== null) ids.add(m[1]);
  return ids;
}

/** Strip then rebuild the linked-note section at the end of a markdown body. */
function bodyWithNoteLinks(baseBody: string, noteIds: string[], titles: Record<string, string>): string {
  const clean = baseBody.replace(/\n\n<!-- linked notes -->[\s\S]*$/, '');
  if (noteIds.length === 0) return clean;
  const lines = noteIds.map((id) => `- [${titles[id] ?? id}](syllepsis://note/${id})`);
  return `${clean}\n\n<!-- linked notes -->\n${lines.join('\n')}`;
}

/**
 * Post-process the exported SVG string to wrap note-linked elements in <a> anchors so the SVG
 * is a valid interactive document even outside the app.
 */
function postProcessSvgLinks(svgString: string, elements: readonly ExcalidrawElement[]): string {
  const linked = elements.filter((el) => {
    const link = (el as ExcalidrawElement & WithLink).link;
    return link?.startsWith('syllepsis://note/');
  });
  if (linked.length === 0) return svgString;

  let result = svgString;
  for (const el of linked) {
    const href = (el as ExcalidrawElement & WithLink).link!;
    const escapedId = el.id.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    // Find the opening <g> for this element and prepend an <a> before it.
    const gTagRe = new RegExp(`(<g[^>]*\\bid="${escapedId}"[^>]*>)`);
    const gMatch = gTagRe.exec(result);
    if (!gMatch) continue;
    const gStart = gMatch.index;
    // Walk forward tracking nesting depth to find the matching </g>.
    let depth = 1;
    let pos = gStart + gMatch[0].length;
    let gEnd = -1;
    while (pos < result.length && depth > 0) {
      const nextOpen = result.indexOf('<g', pos);
      const nextClose = result.indexOf('</g>', pos);
      if (nextClose === -1) break;
      if (nextOpen !== -1 && nextOpen < nextClose) {
        depth++;
        pos = nextOpen + 2;
      } else {
        depth--;
        if (depth === 0) { gEnd = nextClose + 4; break; }
        pos = nextClose + 4;
      }
    }
    if (gEnd === -1) continue;
    const anchor = `<a xmlns:xlink="http://www.w3.org/1999/xlink" xlink:href="${href}">`;
    result =
      result.slice(0, gStart) +
      anchor +
      result.slice(gStart, gEnd) +
      '</a>' +
      result.slice(gEnd);
  }
  return result;
}

/** Catches render-phase errors from the Excalidraw canvas so a single failure renders an
 *  inline message instead of white-screening the entire app, and surfaces the real error
 *  (useful for diagnosing webview-specific failures). */
class CanvasErrorBoundary extends Component<
  { children: ReactNode },
  { error: Error | null }
> {
  state: { error: Error | null } = { error: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // eslint-disable-next-line no-console
    console.error('Excalidraw canvas crashed:', error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div className="drawing-editor-error">
          <div className="drawing-editor-viewonly-body">
            <strong className="drawing-editor-viewonly-title">The drawing canvas failed to load</strong>
            <code className="drawing-editor-viewonly-reason">
              {this.state.error.message || String(this.state.error)}
            </code>
            <span className="drawing-editor-viewonly-detail">
              {this.state.error.stack}
            </span>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}

export function DrawingEditor({ note, markDirty, getSvgRef, onSaved }: Props) {
  const { openEditor } = useStore();
  const [initialData, setInitialData] = useState<ExcalidrawInitialDataState | null>(null);
  const [isViewOnly, setIsViewOnly] = useState(false);
  const [viewOnlyReason, setViewOnlyReason] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [linkPickerOpen, setLinkPickerOpen] = useState(false);
  const [allNotes, setAllNotes] = useState<NoteDto[]>([]);

  const excalidrawApiRef = useRef<ExcalidrawImperativeAPI | null>(null);
  const latestElementsRef = useRef<readonly ExcalidrawElement[]>([]);
  const latestAppStateRef = useRef<AppState | null>(null);
  const latestFilesRef = useRef<BinaryFiles>({});
  const lastSavedLinkIdsRef = useRef<Set<string>>(parseLinkedNoteIds(note.body));
  // Hash of the last scene we acted on. Excalidraw fires onChange very frequently — including
  // on re-renders and after a save re-renders us — so we only mark the note dirty when the
  // element content actually changed, preventing perpetual no-op autosaves.
  const lastSceneHashRef = useRef<number | null>(null);

  // Load scene from the SVG asset on mount / note change.
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setInitialData(null);
    setIsViewOnly(false);
    setViewOnlyReason(null);
    api.readDrawingSvg(note.id)
      .then(async (svg) => {
        if (cancelled) return;
        const sceneJson = extractSceneFromSvg(svg);
        if (!sceneJson) {
          setIsViewOnly(true);
          setViewOnlyReason('No Excalidraw scene found in SVG metadata.');
          setLoading(false);
          return;
        }
        try {
          const parsed = JSON.parse(sceneJson) as {
            elements?: ExcalidrawElement[];
            appState?: Partial<AppState>;
            files?: BinaryFiles;
          };
          const data: ExcalidrawInitialDataState = {
            elements: parsed.elements ?? [],
            appState: parsed.appState ?? {},
            files: parsed.files ?? {},
          };
          if (!cancelled) {
            setInitialData(data);
            latestElementsRef.current = data.elements ?? [];
            lastSceneHashRef.current = hashElementsVersion(data.elements ?? []);
          }
        } catch (e) {
          if (!cancelled) {
            setIsViewOnly(true);
            setViewOnlyReason(String(e));
          }
        }
        if (!cancelled) setLoading(false);
      })
      .catch((e: unknown) => {
        if (!cancelled) { setError(String(e)); setLoading(false); }
      });
    return () => { cancelled = true; };
  }, [note.id]);

  useEffect(() => {
    api.listNotes().then(setAllNotes).catch(() => {});
  }, [note.id]);

  // Expose getSvg to the Editor's save path.
  useEffect(() => {
    getSvgRef.current = async () => {
      if (!excalidrawApiRef.current || isViewOnly) return null;
      try {
        const elements = latestElementsRef.current;
        const appState = latestAppStateRef.current ?? {};
        const files = latestFilesRef.current;
        const svgEl = await exportToSvg({ elements, appState, files });
        // Inject the scene JSON into <metadata> so it can be restored on reopen.
        // exportToSvg does not embed the scene itself — we do it manually.
        const sceneJson = serializeAsJSON(elements, appState, files, 'local');
        let metaEl = svgEl.querySelector('metadata');
        if (!metaEl) {
          metaEl = document.createElementNS('http://www.w3.org/2000/svg', 'metadata');
          svgEl.insertBefore(metaEl, svgEl.firstChild);
        }
        metaEl.textContent = sceneJson;
        const svgString = new XMLSerializer().serializeToString(svgEl);
        return postProcessSvgLinks(svgString, elements);
      } catch {
        return null;
      }
    };
  }, [getSvgRef, isViewOnly]);

  // Sync the body's linked-note list when the link set changes.
  const syncBodyLinks = useCallback(async (elements: readonly ExcalidrawElement[]) => {
    const currentIds = noteLinksFromElements(elements);
    const lastIds = lastSavedLinkIdsRef.current;
    const changed =
      currentIds.size !== lastIds.size ||
      [...currentIds].some((id) => !lastIds.has(id));
    if (!changed) return;
    lastSavedLinkIdsRef.current = new Set(currentIds);
    const sortedIds = [...currentIds].sort();
    const noteTitles: Record<string, string> = {};
    for (const id of sortedIds) {
      const n = allNotes.find((x) => x.id === id);
      if (n) noteTitles[id] = n.title;
    }
    const baseBody = note.body.replace(/\n\n<!-- linked notes -->[\s\S]*$/, '');
    const newBody = bodyWithNoteLinks(baseBody, sortedIds, noteTitles);
    if (newBody !== note.body) {
      const updated = await api.updateNote({ ...note, body: newBody });
      onSaved?.(updated);
    }
  }, [allNotes, note, onSaved]);

  // Store syncBodyLinks on the ref so Editor.tsx can access it post-save.
  const syncBodyLinksRef = useRef(syncBodyLinks);
  syncBodyLinksRef.current = syncBodyLinks;
  useEffect(() => {
    (getSvgRef as unknown as { _syncLinks: () => Promise<void> })._syncLinks = () =>
      syncBodyLinksRef.current(latestElementsRef.current);
  }, [getSvgRef]);

  const handleChange = useCallback(
    (elements: readonly ExcalidrawElement[], appState: AppState, files: BinaryFiles) => {
      latestElementsRef.current = elements;
      latestAppStateRef.current = appState;
      latestFilesRef.current = files;
      // Only flag the note dirty (and thus schedule an autosave) when the drawing's elements
      // actually changed — not on the spurious onChange events Excalidraw emits on re-render.
      const hash = hashElementsVersion(elements);
      if (hash === lastSceneHashRef.current) return;
      lastSceneHashRef.current = hash;
      markDirty();
    },
    [markDirty],
  );

  // Must be a stable reference: Excalidraw invokes this inside an effect keyed on the
  // callback identity and calls setState there, so an inline arrow (new identity every
  // render) re-runs that effect on each parent re-render and triggers an infinite update
  // loop ("Maximum update depth exceeded").
  const handleExcalidrawApi = useCallback((a: ExcalidrawImperativeAPI) => {
    excalidrawApiRef.current = a;
  }, []);

  const handleLinkNote = useCallback(
    (targetNote: NoteDto) => {
      setLinkPickerOpen(false);
      const exApi = excalidrawApiRef.current;
      if (!exApi) return;
      const state = exApi.getAppState();
      const selectedIds = Object.keys(state.selectedElementIds ?? {});
      if (selectedIds.length === 0) return;
      const elements = exApi.getSceneElements() as ExcalidrawElement[];
      const updated = elements.map((el: ExcalidrawElement) =>
        selectedIds.includes(el.id)
          ? { ...el, link: `syllepsis://note/${targetNote.id}` }
          : el,
      );
      exApi.updateScene({ elements: updated });
      markDirty();
    },
    [markDirty],
  );

  const handleLinkOpen = useCallback(
    (element: NonDeletedExcalidrawElement, event: CustomEvent) => {
      const link = (element as NonDeletedExcalidrawElement & WithLink).link;
      if (link?.startsWith('syllepsis://note/')) {
        event.preventDefault();
        const id = link.slice('syllepsis://note/'.length);
        if (id) openEditor(id);
      }
    },
    [openEditor],
  );

  if (loading) return <div className="drawing-editor-loading">Loading canvas…</div>;
  if (error) return <div className="drawing-editor-error">{error}</div>;

  if (isViewOnly) {
    return (
      <div className="drawing-editor-viewonly">
        <div className="drawing-editor-viewonly-body">
          <strong className="drawing-editor-viewonly-title">No editable drawing data</strong>
          {viewOnlyReason && (
            <code className="drawing-editor-viewonly-reason">{viewOnlyReason}</code>
          )}
          <span className="drawing-editor-viewonly-detail">
            This SVG has no embedded Excalidraw scene, so it can't be edited here.
            It was likely imported from an external source rather than created in Syllepsis.
          </span>
        </div>
      </div>
    );
  }

  return (
    <div className="drawing-editor-wrap">
      <div className="drawing-editor-chrome">
        <button
          className="drawing-link-note-btn editor-tool-btn"
          onClick={() => setLinkPickerOpen(true)}
          title="Link a note to the selected canvas element"
        >
          Link Note
        </button>
      </div>
      <div className="drawing-editor-canvas">
        <CanvasErrorBoundary>
          <Excalidraw
            initialData={initialData}
            excalidrawAPI={handleExcalidrawApi}
            onChange={handleChange}
            onLinkOpen={handleLinkOpen}
          />
        </CanvasErrorBoundary>
      </div>
      {linkPickerOpen && (
        <NotePicker
          notes={allNotes.filter((n) => n.id !== note.id)}
          onSelect={handleLinkNote}
          onClose={() => setLinkPickerOpen(false)}
        />
      )}
    </div>
  );
}

function NotePicker({
  notes,
  onSelect,
  onClose,
}: {
  notes: NoteDto[];
  onSelect: (note: NoteDto) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState('');
  const filtered = notes.filter((n) => {
    const q = query.trim().toLowerCase();
    return !q || n.title.toLowerCase().includes(q) || n.id.toLowerCase().includes(q);
  });

  return (
    <div className="editor-dialog-backdrop" role="presentation" onClick={onClose}>
      <div
        className="editor-dialog"
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="editor-dialog-header">
          <h3>Link a note to the selected element</h3>
          <button onClick={onClose} aria-label="Close">✕</button>
        </div>
        <div className="editor-dialog-body">
          <input
            className="editor-dialog-search"
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search notes…"
          />
          <div className="editor-merge-list">
            {filtered.slice(0, 40).map((n) => (
              <button
                key={n.id}
                className="editor-merge-row"
                style={{ display: 'block', width: '100%', textAlign: 'left' }}
                onClick={() => onSelect(n)}
              >
                <strong>{n.title || n.id}</strong>
                {n.summary && <small style={{ display: 'block' }}>{n.summary}</small>}
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
