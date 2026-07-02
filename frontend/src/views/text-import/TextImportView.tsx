import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { api } from '../../lib/api';
import { useStore } from '../../lib/store';
import { Icon } from '../../components/Icon';
import { PageHeader } from '../../components/PageHeader';
import type {
  ImportLlmJobResult,
  LlmRouteStatus,
  NoteDto,
  PluginDescriptor,
  TextImportLlmChunk,
  TextImportOptions,
  TextImportPlacement,
  TextImportPreview,
  TextImportPreviewItem,
} from '../../types';
import { ImportControls, TEXT_SOURCE, type PlacementMode } from './ImportControls';
import { PreviewItemCard, type EditableImportItem } from './PreviewItemCard';
import { CategorySuggestPanel } from './CategorySuggestPanel';
import { LlmImportPanel, type ChunkSide } from './LlmImportPanel';
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
  outline_promote_min_lines: 6,
  outline_promote_min_chars: 400,
  outline_promote_max_depth: 3,
  outline_category_min_children: 2,
  outline_group_loose_lines: true,
};

const PROMPT_OVERRIDE_KEY = 'syllepsis.import.promptOverride';
const DEFAULT_CHUNK_MAX_CHARS = 6000;
/** Previews beyond this many items render compact cards behind a "show all" gate. */
const LARGE_PREVIEW_THRESHOLD = 800;
const RENDER_CAP = 200;

export function TextImportView() {
  const { categories, setCategories, setUnsortedCount, openEditor } = useStore();
  const [sourceText, setSourceText] = useState('');
  const [options, setOptions] = useState<TextImportOptions>(DEFAULT_OPTIONS);
  const [preview, setPreview] = useState<TextImportPreview | null>(null);
  const [items, setItems] = useState<EditableImportItem[]>([]);
  const [notes, setNotes] = useState<NoteDto[]>([]);
  const [importPlugins, setImportPlugins] = useState<PluginDescriptor[]>([]);
  const [source, setSource] = useState<string>(TEXT_SOURCE);
  const [placementMode, setPlacementMode] = useState<PlacementMode>('unsorted');
  const [placementCategory, setPlacementCategory] = useState('');
  const [placementNoteId, setPlacementNoteId] = useState('');
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [showAll, setShowAll] = useState(false);

  // ── AI-assisted chunk splitting state ──
  const [llmEnabled, setLlmEnabled] = useState(false);
  const [routeStatuses, setRouteStatuses] = useState<LlmRouteStatus[]>([]);
  const [promptOverride, setPromptOverride] = useState(
    () => localStorage.getItem(PROMPT_OVERRIDE_KEY) ?? '',
  );
  const [chunkMaxChars, setChunkMaxChars] = useState(DEFAULT_CHUNK_MAX_CHARS);
  const [chunks, setChunks] = useState<TextImportLlmChunk[] | null>(null);
  const [llmJobs, setLlmJobs] = useState<Record<number, ImportLlmJobResult>>({});
  const [chunkSides, setChunkSides] = useState<Record<number, ChunkSide>>({});
  const [proposalChecks, setProposalChecks] = useState<Record<number, boolean[]>>({});
  const [llmRunning, setLlmRunning] = useState(false);
  const [llmProgress, setLlmProgress] = useState<string | null>(null);
  const llmCancelRef = useRef(false);

  useEffect(() => {
    api.listNotes().then(setNotes).catch(console.error);
    api
      .listPlugins()
      .then((plugins) => setImportPlugins(plugins.filter((p) => p.kind === 'import_source')))
      .catch(console.error);
    api.llmRouteStatuses().then(setRouteStatuses).catch(console.error);
  }, []);

  useEffect(() => {
    localStorage.setItem(PROMPT_OVERRIDE_KEY, promptOverride);
  }, [promptOverride]);

  const importSplitRoute = useMemo(
    () => routeStatuses.find((route) => route.task === 'import_split'),
    [routeStatuses],
  );

  const activePlugin = useMemo(
    () => importPlugins.find((p) => p.id === source) ?? null,
    [importPlugins, source],
  );

  const resetLlmState = useCallback(() => {
    setChunks(null);
    setLlmJobs({});
    setChunkSides({});
    setProposalChecks({});
    llmCancelRef.current = true;
    api.clearImportLlmJobs().catch(() => undefined);
  }, []);

  const applyPreview = useCallback((nextPreview: TextImportPreview) => {
    setPreview(nextPreview);
    setItems(nextPreview.items.map((item) => ({ ...item, accepted: true })));
    setShowAll(false);
    resetLlmState();
  }, [resetLlmState]);

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

  const choosePluginFile = useCallback(async () => {
    if (!activePlugin) return;
    const exts = activePlugin.import_extensions.length > 0 ? activePlugin.import_extensions : ['*'];
    const picked = await openDialog({
      multiple: false,
      title: `Choose a ${activePlugin.name} file`,
      filters: [{ name: activePlugin.name, extensions: exts }, { name: 'All files', extensions: ['*'] }],
    });
    if (!picked || typeof picked !== 'string') return;
    setBusy(true);
    try {
      const nextPreview = await api.previewPluginImport(activePlugin.id, picked, options);
      applyPreview(nextPreview);
      setError(null);
      setNotice(
        `${activePlugin.name}: previewed ${nextPreview.items.length} note${nextPreview.items.length === 1 ? '' : 's'} from ${picked.split('/').pop() ?? 'file'}.`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, [activePlugin, applyPreview, options]);

  const runPreview = useCallback(async () => {
    if (!sourceText.trim()) {
      setError('Paste or choose text before previewing.');
      return;
    }
    setBusy(true);
    try {
      const nextPreview = await api.previewTextImport(sourceText, options);
      applyPreview(nextPreview);
      setError(null);
      setNotice(`Previewed ${nextPreview.items.length} note${nextPreview.items.length === 1 ? '' : 's'}.`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, [applyPreview, options, sourceText]);

  const updateOption = useCallback(<K extends keyof TextImportOptions>(key: K, value: TextImportOptions[K]) => {
    setOptions((current) => ({ ...current, [key]: value }));
  }, []);

  const updateItemAt = useCallback((position: number, patch: Partial<EditableImportItem>) => {
    setItems((current) => current.map((item, index) => (index === position ? { ...item, ...patch } : item)));
  }, []);

  const removeItemAt = useCallback((position: number) => {
    setItems((current) => current.filter((_, index) => index !== position));
    // Chunk item positions are now stale; the AI panel starts over.
    resetLlmState();
  }, [resetLlmState]);

  const applySuggestion = useCallback((name: string, itemPositions: number[]) => {
    const positions = new Set(itemPositions);
    setItems((current) =>
      current.map((item, index) => {
        if (!positions.has(index)) return item;
        if (item.categories.includes(name) || item.category_context === name) return item;
        return { ...item, categories: [...item.categories, name] };
      }),
    );
    setNotice(`Added #${name} to ${itemPositions.length} note${itemPositions.length === 1 ? '' : 's'}.`);
  }, []);

  // ── AI chunk running (sequential, 1s polling) ──

  const backendItems = useMemo<TextImportPreviewItem[]>(
    () => items.map(({ accepted: _accepted, ...item }) => item),
    [items],
  );

  useEffect(() => {
    if (!llmEnabled || items.length === 0) return;
    let cancelled = false;
    api
      .chunkTextImportForLlm(backendItems, chunkMaxChars)
      .then((nextChunks) => {
        if (!cancelled) setChunks(nextChunks);
      })
      .catch((e) => setError(String(e)));
    return () => {
      cancelled = true;
    };
    // Re-chunk only when the panel is enabled, the size changes, or the item count changes —
    // title/body edits keep positions stable and existing job results meaningful.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [llmEnabled, chunkMaxChars, items.length]);

  const runChunkList = useCallback(async (chunkList: TextImportLlmChunk[]) => {
    if (chunkList.length === 0) return;
    setLlmRunning(true);
    llmCancelRef.current = false;
    try {
      for (let i = 0; i < chunkList.length; i += 1) {
        if (llmCancelRef.current) break;
        const chunk = chunkList[i];
        setLlmProgress(`chunk ${i + 1}/${chunkList.length}`);
        let job = await api.enqueueImportLlmChunk({
          chunk_index: chunk.index,
          heading: chunk.heading ?? null,
          text: chunk.text,
          model_override: null,
          prompt_override: promptOverride.trim() ? promptOverride : null,
        });
        setLlmJobs((current) => ({ ...current, [chunk.index]: job }));
        while (job.status === 'queued' || job.status === 'running') {
          await sleep(1000);
          const next = await api.getImportLlmJob(job.job_id);
          if (next) {
            job = next;
            setLlmJobs((current) => ({ ...current, [chunk.index]: next }));
          }
        }
        if (job.status === 'complete' && job.proposed.length > 0) {
          setProposalChecks((current) =>
            current[chunk.index] ? current : { ...current, [chunk.index]: job.proposed.map(() => true) },
          );
        }
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLlmRunning(false);
      setLlmProgress(null);
    }
  }, [promptOverride]);

  const runAllChunks = useCallback(() => {
    if (!chunks) return;
    const pending = chunks.filter((chunk) => llmJobs[chunk.index]?.status !== 'complete');
    void runChunkList(pending.length > 0 ? pending : chunks);
  }, [chunks, llmJobs, runChunkList]);

  const runOneChunk = useCallback((chunkIndex: number) => {
    const chunk = chunks?.find((c) => c.index === chunkIndex);
    if (chunk) void runChunkList([chunk]);
  }, [chunks, runChunkList]);

  const cancelLlmRun = useCallback(() => {
    llmCancelRef.current = true;
  }, []);

  // ── Commit assembly: per-chunk side selection decides which items land ──

  const chunkByPosition = useMemo(() => {
    const map = new Map<number, TextImportLlmChunk>();
    chunks?.forEach((chunk) => chunk.item_indices.forEach((position) => map.set(position, chunk)));
    return map;
  }, [chunks]);

  const commitItems = useMemo<TextImportPreviewItem[]>(() => {
    const out: TextImportPreviewItem[] = [];
    const consumedChunks = new Set<number>();
    items.forEach((item, position) => {
      const chunk = chunkByPosition.get(position);
      const job = chunk ? llmJobs[chunk.index] : undefined;
      const aiWins =
        chunk !== undefined
        && (chunkSides[chunk.index] ?? 'deterministic') === 'ai'
        && job?.status === 'complete'
        && job.proposed.length > 0;
      if (chunk && aiWins && job) {
        if (consumedChunks.has(chunk.index)) return;
        consumedChunks.add(chunk.index);
        const checks = proposalChecks[chunk.index] ?? job.proposed.map(() => true);
        const firstDeterministic = items[chunk.item_indices[0]];
        const startsCategory = firstDeterministic?.intended_prior?.target === 'category';
        job.proposed.forEach((proposal, proposalIndex) => {
          if (!checks[proposalIndex]) return;
          const heading = chunk.heading ?? null;
          out.push({
            index: 0,
            title: proposal.title || 'Imported note',
            body: proposal.body,
            block_kind: 'paragraph',
            category_context: heading,
            intended_prior:
              heading && startsCategory && !out.some((existing) => existing.category_context === heading)
                ? { target: 'category', target_label: heading, kind: 'new_paragraph' }
                : { target: 'previous_imported_note', target_label: null, kind: 'new_paragraph' },
            warnings: [],
            categories: proposal.categories.filter((category) => category !== heading),
            parent_index: null,
            depth: 0,
          });
        });
        return;
      }
      if (item.accepted) {
        const { accepted: _accepted, ...plain } = item;
        out.push(plain);
      }
    });
    return out.map((item, index) => ({ ...item, index }));
  }, [chunkByPosition, chunkSides, items, llmJobs, proposalChecks]);

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
    || llmRunning
    || !preview
    || commitItems.length === 0
    || (placementMode === 'category' && !placementCategory.trim())
    || (placementMode === 'after_note' && !placementNoteId);

  const commitImport = useCallback(async () => {
    if (!preview || commitDisabled) return;
    setBusy(true);
    try {
      const report = await api.commitTextImport({
        items: commitItems,
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
  }, [commitDisabled, commitItems, openEditor, placement, preview, setCategories, setUnsortedCount]);

  const existingCategoryNames = useMemo(() => categories.map((c) => c.name), [categories]);
  const knownCategoryNames = useMemo(() => {
    const names = new Set(existingCategoryNames);
    items.forEach((item) => item.categories.forEach((category) => names.add(category)));
    preview?.categories.forEach((category) => names.add(category.name));
    return [...names].sort();
  }, [existingCategoryNames, items, preview]);

  const largePreview = items.length > LARGE_PREVIEW_THRESHOLD;
  const visibleItems = largePreview && !showAll ? items.slice(0, RENDER_CAP) : items;

  return (
    <div className="ti-root">
      <PageHeader
        title="Note Import"
        secondary={
          <p className="ti-subtitle">Bring in text from a paste, a file, or an import plugin, preview the split, then import as notes.</p>
        }
      />

      {notice && <div className="ti-notice" onClick={() => setNotice(null)}>{notice}</div>}
      {error && <div className="ti-error" onClick={() => setError(null)}>{error}</div>}

      <div className="ti-body">
        <section className="ti-panel ti-controls-pane">
          <ImportControls
            source={source}
            importPlugins={importPlugins}
            activePlugin={activePlugin}
            onSourceChange={(nextSource) => {
              setSource(nextSource);
              setPreview(null);
              setItems([]);
              resetLlmState();
            }}
            sourceText={sourceText}
            onSourceTextChange={setSourceText}
            onChooseFile={chooseFile}
            onChoosePluginFile={choosePluginFile}
            options={options}
            onOptionChange={updateOption}
            placementMode={placementMode}
            onPlacementModeChange={setPlacementMode}
            placementCategory={placementCategory}
            onPlacementCategoryChange={setPlacementCategory}
            placementNoteId={placementNoteId}
            onPlacementNoteIdChange={setPlacementNoteId}
            categories={categories}
            notes={notes}
            busy={busy}
            onPreview={runPreview}
            commitDisabled={commitDisabled}
            commitCount={commitItems.length}
            onCommit={commitImport}
          />
        </section>

        <section className="ti-panel ti-preview-pane">
          {preview ? (
            <>
              <div className="ti-preview-header">
                <div>
                  <h3>Preview</h3>
                  <p>
                    {commitItems.length} note{commitItems.length === 1 ? '' : 's'} ready.{' '}
                    {preview.categories.length} detected section{preview.categories.length === 1 ? '' : 's'}.
                  </p>
                </div>
              </div>

              <div className="ti-preview-scroll">
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

                <CategorySuggestPanel
                  items={backendItems}
                  existingCategories={existingCategoryNames}
                  onApply={applySuggestion}
                  onError={setError}
                />

                <LlmImportPanel
                  enabled={llmEnabled}
                  onEnabledChange={setLlmEnabled}
                  routeStatus={importSplitRoute}
                  promptOverride={promptOverride}
                  onPromptOverrideChange={setPromptOverride}
                  chunkMaxChars={chunkMaxChars}
                  onChunkMaxCharsChange={setChunkMaxChars}
                  chunks={chunks}
                  jobs={llmJobs}
                  chunkSides={chunkSides}
                  proposalChecks={proposalChecks}
                  running={llmRunning}
                  progress={llmProgress}
                  items={items}
                  onRunAll={runAllChunks}
                  onRunChunk={runOneChunk}
                  onCancel={cancelLlmRun}
                  onSideChange={(chunkIndex, side) =>
                    setChunkSides((current) => ({ ...current, [chunkIndex]: side }))
                  }
                  onProposalCheckChange={(chunkIndex, proposalIndex, checked) =>
                    setProposalChecks((current) => {
                      const jobProposals = llmJobs[chunkIndex]?.proposed ?? [];
                      const checks = [...(current[chunkIndex] ?? jobProposals.map(() => true))];
                      checks[proposalIndex] = checked;
                      return { ...current, [chunkIndex]: checks };
                    })
                  }
                />

                <datalist id="ti-known-categories">
                  {knownCategoryNames.map((category) => (
                    <option key={category} value={category} />
                  ))}
                </datalist>

                <div className="ti-preview-list">
                  {visibleItems.map((item, position) => (
                    <PreviewItemCard
                      key={`${item.index}-${position}`}
                      item={item}
                      displayNumber={position + 1}
                      compact={largePreview}
                      onChange={(patch) => updateItemAt(position, patch)}
                      onRemove={() => removeItemAt(position)}
                    />
                  ))}
                </div>

                {largePreview && !showAll && (
                  <button className="ti-btn ti-show-all" onClick={() => setShowAll(true)}>
                    Show all {items.length} items
                  </button>
                )}
              </div>
            </>
          ) : (
            <div className="ti-preview-empty">
              <Icon name="visibility" size={32} />
              <p>Preview will appear here once you run an import.</p>
            </div>
          )}
        </section>
      </div>
    </div>
  );
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
