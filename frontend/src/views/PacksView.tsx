// Knowledge packs (Phase 6, core-concepts.md): export a curated set of notes as a distributable
// pack, or import one into this book with category mapping and local-modification protection.

import { useCallback, useMemo, useState } from 'react';
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import { PageHeader } from '../components/PageHeader';
import type { ImportPreview, ImportReport, BookInfo } from '../types';
import './PacksView.css';

const PACK_FILTER = [{ name: 'Syllepsis pack', extensions: ['synpack.json', 'json'] }];

const STATUS_LABEL: Record<string, string> = {
  new: 'new',
  update: 'update',
  locally_modified: 'locally edited — will be skipped',
};

export function PacksView() {
  const { categories } = useStore();
  const [tab, setTab] = useState<'export' | 'import'>('export');
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  return (
    <div className="pk-root">
      <PageHeader title="Knowledge Packs">
        <div className="pk-tabs">
          <button className={`pk-tab ${tab === 'export' ? 'active' : ''}`} onClick={() => setTab('export')}>Export</button>
          <button className={`pk-tab ${tab === 'import' ? 'active' : ''}`} onClick={() => setTab('import')}>Import</button>
        </div>
      </PageHeader>

      {notice && <div className="pk-notice" onClick={() => setNotice(null)}>{notice}</div>}
      {error && <div className="pk-state pk-error" onClick={() => setError(null)}>{error}</div>}

      {tab === 'export' ? (
        <ExportPanel categories={categories.map((c) => c.name)} onNotice={setNotice} onError={setError} />
      ) : (
        <ImportPanel localCategories={categories.map((c) => c.name)} onNotice={setNotice} onError={setError} />
      )}
    </div>
  );
}

interface PanelProps {
  onNotice: (m: string) => void;
  onError: (m: string) => void;
}

function ExportPanel({ categories, onNotice, onError }: PanelProps & { categories: string[] }) {
  const { book } = useStore();
  const [id, setId] = useState('');
  const [name, setName] = useState(() => book?.name ?? '');
  const [version, setVersion] = useState('1.0.0');
  const [description, setDescription] = useState('');
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [exportAll, setExportAll] = useState(false);
  const [busy, setBusy] = useState(false);

  const toggle = (cat: string) => setSelected((prev) => {
    const next = new Set(prev);
    if (next.has(cat)) next.delete(cat); else next.add(cat);
    return next;
  });

  const exportPack = useCallback(async () => {
    if (!id.trim() || !name.trim()) { onError('Pack needs an id and a name.'); return; }
    if (!exportAll && selected.size === 0) { onError('Select at least one category to export.'); return; }
    const path = await saveDialog({ title: 'Save pack as…', defaultPath: `${id.trim()}.synpack.json`, filters: PACK_FILTER });
    if (!path) return;
    setBusy(true);
    try {
      const manifest = await api.exportPack(
        { id: id.trim(), name: name.trim(), version: version.trim() || '1.0.0', description, categories: [...selected], note_ids: [], export_all: exportAll },
        path,
      );
      onNotice(`Exported “${manifest.name}” v${manifest.version}.`);
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  }, [id, name, version, description, selected, exportAll, onNotice, onError]);

  return (
    <section className="pk-panel">
      <p className="pk-hint">Bundle every note in the chosen categories into a single distributable file.</p>
      <label className="pk-field"><span>Pack id</span>
        <input value={id} onChange={(e) => setId(e.target.value)} placeholder="permaculture-basics" /></label>
      <label className="pk-field"><span>Name</span>
        <input value={name} onChange={(e) => setName(e.target.value)} placeholder="Permaculture Basics" /></label>
      <label className="pk-field"><span>Version</span>
        <input value={version} onChange={(e) => setVersion(e.target.value)} placeholder="1.0.0" /></label>
      <label className="pk-field"><span>Description</span>
        <textarea value={description} onChange={(e) => setDescription(e.target.value)} rows={2} /></label>

      <label className="pk-check pk-export-all-toggle">
        <input type="checkbox" checked={exportAll} onChange={(e) => setExportAll(e.target.checked)} />
        <span>Export entire book (all non-deleted notes)</span>
      </label>

      <div className="pk-subhead">Categories to include</div>
      {categories.length === 0 && <div className="pk-state">No categories in this book yet.</div>}
      <div className={`pk-checklist${exportAll ? ' pk-checklist-disabled' : ''}`}>
        {categories.map((cat) => (
          <label key={cat} className="pk-check">
            <input type="checkbox" checked={selected.has(cat)} onChange={() => toggle(cat)} disabled={exportAll} />
            <span>#{cat}</span>
          </label>
        ))}
      </div>

      <button className="pk-btn pk-btn-primary" disabled={busy} onClick={exportPack}>
        {exportAll ? 'Export book…' : 'Export pack…'}
      </button>
    </section>
  );
}

function ImportPanel({ localCategories, onNotice, onError }: PanelProps & { localCategories: string[] }) {
  const { setBook, setCategories } = useStore();
  const [path, setPath] = useState<string | null>(null);
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [importMode, setImportMode] = useState<'merge' | 'new_book' | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [map, setMap] = useState<Record<string, string>>({});
  const [report, setReport] = useState<ImportReport | null>(null);
  const [busy, setBusy] = useState(false);

  const choose = useCallback(async () => {
    const picked = await openDialog({ multiple: false, title: 'Choose a pack file', filters: PACK_FILTER });
    if (!picked || typeof picked !== 'string') return;
    setBusy(true);
    setReport(null);
    try {
      const pv = await api.previewPack(picked);
      setPath(picked);
      setPreview(pv);
      setImportMode(pv.manifest.export_kind === 'book' ? 'new_book' : 'merge');
      setSelected(new Set(pv.notes.map((n) => n.id)));
      const initialMap: Record<string, string> = {};
      for (const m of pv.category_suggestions) if (m.suggested_local) initialMap[m.incoming] = m.suggested_local;
      setMap(initialMap);
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  }, [onError]);

  const resetImport = () => {
    setPath(null);
    setPreview(null);
    setImportMode(null);
    setSelected(new Set());
    setMap({});
    setReport(null);
  };

  const toggle = (noteId: string) => setSelected((prev) => {
    const next = new Set(prev);
    if (next.has(noteId)) next.delete(noteId); else next.add(noteId);
    return next;
  });

  const setMapping = (incoming: string, local: string) => setMap((prev) => {
    const next = { ...prev };
    if (local) next[incoming] = local; else delete next[incoming];
    return next;
  });

  const runImport = useCallback(async () => {
    if (!path) return;
    setBusy(true);
    try {
      const r = await api.importPack(path, { selected_note_ids: [...selected], category_map: map });
      setReport(r);
      onNotice(`Imported ${r.imported.length}, skipped ${r.skipped_locally_modified.length} locally-edited.`);
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  }, [path, selected, map, onNotice, onError]);

  const runImportAsBook = useCallback(async () => {
    if (!path || !preview) return;
    const parentDir = await openDialog({ directory: true, title: 'Choose folder for new book' });
    if (!parentDir || typeof parentDir !== 'string') return;
    const bookName = prompt('Name for the new book:', preview.manifest.name);
    if (!bookName?.trim()) return;
    setBusy(true);
    try {
      const info: BookInfo = await api.importPackAsBook(path, parentDir, bookName.trim());
      setBook(info);
      const cats = await api.allCategories();
      setCategories(cats);
      onNotice(`Created book "${info.name}" from pack.`);
      resetImport();
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  }, [path, preview, setBook, setCategories, onNotice, onError]);

  const selectableCount = useMemo(() => preview?.notes.length ?? 0, [preview]);

  return (
    <section className="pk-panel">
      <p className="pk-hint">Load a pack into this book. Locally-edited pack notes are protected from being overwritten.</p>
      <button className="pk-btn" disabled={busy} onClick={choose}>Choose pack file…</button>

      {preview && (
        <>
          <div className="pk-manifest">
            <strong>{preview.manifest.name}</strong> v{preview.manifest.version}
            {preview.manifest.description && <div className="pk-hint">{preview.manifest.description}</div>}
          </div>

          <div className="pk-choice-row">
            <label className={`pk-choice-option${importMode === 'merge' ? ' active' : ''}`}>
              <input type="radio" name="import-mode" value="merge" checked={importMode === 'merge'} onChange={() => setImportMode('merge')} />
              Merge into current book
            </label>
            <label className={`pk-choice-option${importMode === 'new_book' ? ' active' : ''}`}>
              <input type="radio" name="import-mode" value="new_book" checked={importMode === 'new_book'} onChange={() => setImportMode('new_book')} />
              Open as new book
            </label>
          </div>

          {importMode === 'new_book' ? (
            <button className="pk-btn pk-btn-primary" disabled={busy} onClick={runImportAsBook}>
              Create new book from this pack…
            </button>
          ) : (
            <>
              <div className="pk-subhead">Notes ({selected.size}/{selectableCount})</div>
              <div className="pk-checklist">
                {preview.notes.map((n) => (
                  <label key={n.id} className={`pk-check pk-status-${n.status}`}>
                    <input type="checkbox" checked={selected.has(n.id)} onChange={() => toggle(n.id)} />
                    <span className="pk-note-title">{n.title || '(untitled)'}</span>
                    <span className="pk-status">{STATUS_LABEL[n.status]}</span>
                  </label>
                ))}
              </div>

              {preview.category_suggestions.length > 0 && (
                <>
                  <div className="pk-subhead">Category mapping</div>
                  {preview.category_suggestions.map((m) => (
                    <div key={m.incoming} className="pk-maprow">
                      <span className="pk-incoming">#{m.incoming}</span>
                      <span className="pk-arrow">→</span>
                      <select value={map[m.incoming] ?? ''} onChange={(e) => setMapping(m.incoming, e.target.value)}>
                        <option value="">create #{m.incoming}</option>
                        {localCategories.map((c) => <option key={c} value={c}>#{c}</option>)}
                      </select>
                    </div>
                  ))}
                </>
              )}

              <button className="pk-btn pk-btn-primary" disabled={busy || selected.size === 0} onClick={runImport}>
                Import {selected.size} note{selected.size !== 1 ? 's' : ''}
              </button>
            </>
          )}
        </>
      )}

      {report && (
        <div className="pk-report">
          Imported {report.imported.length}. Skipped {report.skipped_locally_modified.length} locally-edited.
          {report.created_categories.length > 0 && <> Created {report.created_categories.length} categor{report.created_categories.length !== 1 ? 'ies' : 'y'}.</>}
        </div>
      )}
    </section>
  );
}
