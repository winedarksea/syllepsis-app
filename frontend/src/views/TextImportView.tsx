import { useCallback, useEffect, useMemo, useState } from 'react';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import { Icon } from '../components/Icon';
import type {
  NoteDto,
  TextImportOptions,
  TextImportPlacement,
  TextImportPreview,
  TextImportPreviewItem,
  TextImportSplitMode,
} from '../types';
import './TextImportView.css';

const TEXT_FILTER = [
  { name: 'Text or Markdown', extensions: ['txt', 'md', 'markdown'] },
  { name: 'All files', extensions: ['*'] },
];

const DEFAULT_OPTIONS: TextImportOptions = {
  split_mode: 'smart',
  detect_headings: true,
  detect_lists: true,
  detect_tables: true,
  detect_code_blocks: true,
  convert_indented_lists: false,
};

const SPLIT_MODES: { value: TextImportSplitMode; label: string }[] = [
  { value: 'smart', label: 'Smart blocks' },
  { value: 'paragraph', label: 'Paragraphs' },
  { value: 'non_empty_line', label: 'Lines' },
  { value: 'one_note', label: 'One note' },
];

const BLOCK_LABEL: Record<string, string> = {
  paragraph: 'Paragraph',
  list: 'List',
  table: 'Table',
  code: 'Code',
};

export function TextImportView() {
  const { categories, setCategories, setUnsortedCount, openEditor } = useStore();
  const [sourceText, setSourceText] = useState('');
  const [options, setOptions] = useState<TextImportOptions>(DEFAULT_OPTIONS);
  const [preview, setPreview] = useState<TextImportPreview | null>(null);
  const [items, setItems] = useState<TextImportPreviewItem[]>([]);
  const [notes, setNotes] = useState<NoteDto[]>([]);
  const [placementMode, setPlacementMode] = useState<'unsorted' | 'category' | 'after_note'>('unsorted');
  const [placementCategory, setPlacementCategory] = useState('');
  const [placementNoteId, setPlacementNoteId] = useState('');
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.listNotes().then(setNotes).catch(console.error);
  }, []);

  const chooseFile = useCallback(async () => {
    const picked = await openDialog({
      multiple: false,
      title: 'Choose text or Markdown file',
      filters: TEXT_FILTER,
    });
    if (!picked || typeof picked !== 'string') return;
    setBusy(true);
    try {
      const text = await api.readTextImportFile(picked);
      setSourceText(text);
      setNotice(`Loaded ${picked.split('/').pop() ?? 'file'}.`);
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const runPreview = useCallback(async () => {
    if (!sourceText.trim()) {
      setError('Paste or choose text before previewing.');
      return;
    }
    setBusy(true);
    try {
      const nextPreview = await api.previewTextImport(sourceText, options);
      setPreview(nextPreview);
      setItems(nextPreview.items);
      setError(null);
      setNotice(`Previewed ${nextPreview.items.length} note${nextPreview.items.length === 1 ? '' : 's'}.`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, [options, sourceText]);

  const updateOption = <K extends keyof TextImportOptions>(key: K, value: TextImportOptions[K]) => {
    setOptions((current) => ({ ...current, [key]: value }));
  };

  const updateItem = (index: number, patch: Partial<TextImportPreviewItem>) => {
    setItems((current) => current.map((item) => (item.index === index ? { ...item, ...patch } : item)));
  };

  const removeItem = (index: number) => {
    setItems((current) => current.filter((item) => item.index !== index));
  };

  const placement = useMemo<TextImportPlacement>(() => {
    if (placementMode === 'category') {
      return { kind: 'category', category: placementCategory };
    }
    if (placementMode === 'after_note') {
      return { kind: 'after_note', note_id: placementNoteId };
    }
    return { kind: 'unsorted' };
  }, [placementCategory, placementMode, placementNoteId]);

  const commitDisabled =
    busy
    || !preview
    || items.length === 0
    || (placementMode === 'category' && !placementCategory.trim())
    || (placementMode === 'after_note' && !placementNoteId);

  const commitImport = useCallback(async () => {
    if (!preview || commitDisabled) return;
    setBusy(true);
    try {
      const report = await api.commitTextImport({
        items: items.map((item, index) => ({ ...item, index })),
        categories: preview.categories,
        placement,
      });
      const [freshCategories, freshUnsorted] = await Promise.all([
        api.allCategories(),
        api.unsortedNotes(),
      ]);
      setCategories(freshCategories);
      setUnsortedCount(freshUnsorted.length);
      setNotice(`Imported ${report.imported.length} note${report.imported.length === 1 ? '' : 's'}.`);
      setError(null);
      if (report.first_note_id) openEditor(report.first_note_id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, [commitDisabled, items, openEditor, placement, preview, setCategories, setUnsortedCount]);

  return (
    <div className="ti-root">
      <div className="ti-header">
        <div>
          <h2 className="ti-title">Text Import</h2>
          <p className="ti-subtitle">Paste or load a long text document, preview the split, then import as notes.</p>
        </div>
        <button className="ti-btn" disabled={busy} onClick={chooseFile}>
          <Icon name="folder_open" size={18} />
          <span>Choose file</span>
        </button>
      </div>

      {notice && <div className="ti-notice" onClick={() => setNotice(null)}>{notice}</div>}
      {error && <div className="ti-error" onClick={() => setError(null)}>{error}</div>}

      <section className="ti-panel">
        <label className="ti-field">
          <span>Source text</span>
          <textarea
            className="ti-source"
            value={sourceText}
            onChange={(event) => setSourceText(event.target.value)}
            placeholder="Paste plain text or Markdown here."
          />
        </label>

        <div className="ti-controls">
          <label className="ti-field">
            <span>Split mode</span>
            <select value={options.split_mode} onChange={(event) => updateOption('split_mode', event.target.value as TextImportSplitMode)}>
              {SPLIT_MODES.map((mode) => (
                <option key={mode.value} value={mode.value}>{mode.label}</option>
              ))}
            </select>
          </label>

          <label className="ti-field">
            <span>Placement</span>
            <select value={placementMode} onChange={(event) => setPlacementMode(event.target.value as typeof placementMode)}>
              <option value="unsorted">Unsorted or detected sections</option>
              <option value="category">Start in category</option>
              <option value="after_note">After existing note</option>
            </select>
          </label>

          {placementMode === 'category' && (
            <label className="ti-field">
              <span>Category</span>
              <select value={placementCategory} onChange={(event) => setPlacementCategory(event.target.value)}>
                <option value="">Choose category</option>
                {categories.map((category) => (
                  <option key={category.name} value={category.name}>#{category.name}</option>
                ))}
              </select>
            </label>
          )}

          {placementMode === 'after_note' && (
            <label className="ti-field">
              <span>Previous note</span>
              <select value={placementNoteId} onChange={(event) => setPlacementNoteId(event.target.value)}>
                <option value="">Choose note</option>
                {notes.map((note) => (
                  <option key={note.id} value={note.id}>{note.title || note.id}</option>
                ))}
              </select>
            </label>
          )}
        </div>

        <div className="ti-toggles">
          <Toggle label="Headings to sections" checked={options.detect_headings} onChange={(value) => updateOption('detect_headings', value)} />
          <Toggle label="Detect lists" checked={options.detect_lists} onChange={(value) => updateOption('detect_lists', value)} />
          <Toggle label="Detect tables" checked={options.detect_tables} onChange={(value) => updateOption('detect_tables', value)} />
          <Toggle label="Detect code fences" checked={options.detect_code_blocks} onChange={(value) => updateOption('detect_code_blocks', value)} />
          <Toggle label="Indented lines as bullets" checked={options.convert_indented_lists} onChange={(value) => updateOption('convert_indented_lists', value)} />
        </div>

        <div className="ti-actions">
          <button className="ti-btn ti-btn-primary" disabled={busy || !sourceText.trim()} onClick={runPreview}>
            <Icon name="visibility" size={18} />
            <span>Preview import</span>
          </button>
          <button className="ti-btn ti-btn-primary" disabled={commitDisabled} onClick={commitImport}>
            <Icon name="upload_file" size={18} />
            <span>Import {items.length || ''} note{items.length === 1 ? '' : 's'}</span>
          </button>
        </div>
      </section>

      {preview && (
        <section className="ti-panel">
          <div className="ti-preview-header">
            <div>
              <h3>Preview</h3>
              <p>{items.length} note{items.length === 1 ? '' : 's'} ready. {preview.categories.length} detected section{preview.categories.length === 1 ? '' : 's'}.</p>
            </div>
          </div>

          {preview.categories.length > 0 && (
            <div className="ti-category-row">
              {preview.categories.map((category) => (
                <span key={category.name} className="ti-chip">#{category.name}</span>
              ))}
            </div>
          )}

          {preview.warnings.map((warning) => (
            <div key={warning} className="ti-warning">{warning}</div>
          ))}

          <div className="ti-preview-list">
            {items.map((item, itemNumber) => (
              <article key={item.index} className="ti-preview-item">
                <div className="ti-preview-item-header">
                  <span className="ti-item-index">{itemNumber + 1}</span>
                  <span className={`ti-kind ti-kind-${item.block_kind}`}>{BLOCK_LABEL[item.block_kind]}</span>
                  {item.category_context && <span className="ti-chip">#{item.category_context}</span>}
                  {item.intended_prior && <span className="ti-prior">{item.intended_prior.kind}</span>}
                  <button className="ti-icon-btn" onClick={() => removeItem(item.index)} title="Remove from import">
                    <Icon name="close" size={17} />
                  </button>
                </div>
                <input
                  className="ti-title-input"
                  value={item.title}
                  onChange={(event) => updateItem(item.index, { title: event.target.value })}
                />
                <textarea
                  className="ti-body-input"
                  value={item.body}
                  onChange={(event) => updateItem(item.index, { body: event.target.value })}
                />
                {item.warnings.map((warning) => (
                  <div key={warning} className="ti-warning">{warning}</div>
                ))}
              </article>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

function Toggle({ label, checked, onChange }: { label: string; checked: boolean; onChange: (value: boolean) => void }) {
  return (
    <label className="ti-toggle">
      <input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} />
      <span>{label}</span>
    </label>
  );
}
