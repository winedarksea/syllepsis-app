// Queued AI writing tools. The editor only enqueues work; completed results are reviewed from the
// app-level job tray so the user can navigate away while local/cloud inference runs.

import { useCallback, useEffect, useMemo, useState } from 'react';
import { api } from '../lib/api';
import { Icon } from '../components/Icon';
import type {
  CloudLlmProviderDescriptor, LlmRouteStatus, LlmTask, ModelRef, QueuedLlmJobResult,
  RewriteMode, StyleCard, StylePerspective, StyleReadingLevel, StyleVerbosity, StyleVoice,
  SummaryVariant,
} from '../types';

const LOCAL_PROVIDER = 'local';

const TASKS: { task: LlmTask; label: string }[] = [
  { task: 'summarize', label: 'Generate summary' },
  { task: 'generate_from_summary', label: 'Generate from summary' },
  { task: 'grammar', label: 'Fix grammar & clarity' },
  { task: 'rewrite', label: 'Rewrite body' },
  { task: 'category_suggest', label: 'Suggest categories' },
  { task: 'fact_check', label: 'Fact-check' },
  { task: 'devils_advocate', label: "Devil's advocate" },
];

const VERBOSITIES: StyleVerbosity[] = ['succinct', 'standard', 'expansive'];
const PERSPECTIVES: StylePerspective[] = [
  'first_person_singular', 'first_person_plural', 'first_person_soliloquy',
  'second_person', 'third_person_objective', 'third_person_omniscient', 'third_person_limited',
];
const READING_LEVELS: StyleReadingLevel[] = ['elementary', 'accessible', 'advanced', 'expert'];
const VOICES: StyleVoice[] = ['active', 'passive'];

function humanize(s: string) {
  return s.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

interface Props {
  noteId: string;
  onQueued?: (job: QueuedLlmJobResult) => void;
}

export function LlmToolsMenu({ noteId, onQueued }: Props) {
  const [open, setOpen] = useState(false);
  const [routes, setRoutes] = useState<LlmRouteStatus[]>([]);
  const [descriptors, setDescriptors] = useState<CloudLlmProviderDescriptor[]>([]);
  const [styleCards, setStyleCards] = useState<StyleCard[]>([]);
  const [task, setTask] = useState<LlmTask>('summarize');

  // Provider override: '' = use route default
  const [overrideProvider, setOverrideProvider] = useState('');
  const [overrideModel, setOverrideModel] = useState('');

  const [summaryVariant, setSummaryVariant] = useState<SummaryVariant>('plain');
  const [rewriteMode, setRewriteMode] = useState<RewriteMode>('standard');
  const [styleCardId, setStyleCardId] = useState('');

  // Structured style overrides (seeded from selected card)
  const [overrideVerbosity, setOverrideVerbosity] = useState<StyleVerbosity | ''>('');
  const [overridePerspective, setOverridePerspective] = useState<StylePerspective | ''>('');
  const [overrideReadingLevel, setOverrideReadingLevel] = useState<StyleReadingLevel | ''>('');
  const [overrideVoice, setOverrideVoice] = useState<StyleVoice | ''>('');
  const [overrideNotes, setOverrideNotes] = useState('');

  const [storeResultAsCommentary, setStoreResultAsCommentary] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    api.llmRouteStatuses().then(setRoutes).catch((e) => setError(String(e)));
    api.cloudLlmProviderDescriptors().then(setDescriptors).catch(() => setDescriptors([]));
    api.listStyleCards().then(setStyleCards).catch(() => setStyleCards([]));
  }, [open]);

  const route = useMemo(
    () => routes.find((candidate) => candidate.task === task) ?? null,
    [routes, task],
  );

  const modelOverride: ModelRef | null = useMemo(() => {
    if (!overrideProvider) return null;
    return { provider: overrideProvider, model: overrideModel };
  }, [overrideProvider, overrideModel]);

  const displayModel = modelOverride ?? (route ? { provider: route.provider, model: route.model } : null);
  const supportsSummaryOptions = task === 'summarize';
  const supportsStyleOptions = task === 'rewrite' || task === 'generate_from_summary';

  // Seed structured overrides when the selected card changes
  useEffect(() => {
    if (!styleCardId) {
      setOverrideVerbosity('');
      setOverridePerspective('');
      setOverrideReadingLevel('');
      setOverrideVoice('');
      return;
    }
    const card = styleCards.find((c) => c.id === styleCardId);
    if (card) {
      setOverrideVerbosity(card.verbosity);
      setOverridePerspective(card.perspective);
      setOverrideReadingLevel(card.reading_level);
      setOverrideVoice(card.voice);
    }
  }, [styleCardId, styleCards]);

  const buildStyleOverrides = (): string | null => {
    const parts: string[] = [];
    if (overrideVerbosity) parts.push(`verbosity: ${overrideVerbosity}`);
    if (overridePerspective) parts.push(`perspective: ${overridePerspective}`);
    if (overrideReadingLevel) parts.push(`reading_level: ${overrideReadingLevel}`);
    if (overrideVoice) parts.push(`voice: ${overrideVoice}`);
    if (overrideNotes.trim()) parts.push(overrideNotes.trim());
    return parts.length > 0 ? parts.join('\n') : null;
  };

  const enqueue = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const job = await api.enqueueLlmJob({
        target_note_id: noteId,
        task,
        model_override: modelOverride,
        style_card_id: supportsStyleOptions && styleCardId ? styleCardId : null,
        style_overrides: supportsStyleOptions ? buildStyleOverrides() : null,
        summary_variant: supportsSummaryOptions ? summaryVariant : 'plain',
        rewrite_mode: supportsStyleOptions ? rewriteMode : 'standard',
        store_result_as_commentary: storeResultAsCommentary,
      });
      onQueued?.(job);
      setOpen(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, [
    modelOverride,
    noteId,
    onQueued,
    rewriteMode,
    storeResultAsCommentary,
    styleCardId,
    summaryVariant,
    supportsStyleOptions,
    supportsSummaryOptions,
    task,
    // eslint-disable-next-line react-hooks/exhaustive-deps
    overrideVerbosity,
    overridePerspective,
    overrideReadingLevel,
    overrideVoice,
    overrideNotes,
  ]);

  return (
    <div className="llm-tools">
      <button className="llm-tools-btn" onClick={() => setOpen(true)} title="AI writing tools">
        <Icon name="auto_awesome" size={16} />
        <span>Tools</span>
      </button>

      {open && (
        <div className="llm-modal-backdrop" role="presentation" onClick={() => setOpen(false)}>
          <div className="llm-modal llm-tool-dialog" role="dialog" aria-modal="true" onClick={(event) => event.stopPropagation()}>
            <div className="llm-modal-header">
              <h3>Run tool</h3>
              {displayModel && (
                <span className="llm-modal-model">{displayModel.provider}/{displayModel.model}</span>
              )}
            </div>
            <div className="llm-tool-form">
              {error && <div className="llm-tools-error inline" onClick={() => setError(null)}>{error}</div>}
              <label className="llm-tool-field">
                <span>Tool</span>
                <select value={task} onChange={(event) => setTask(event.target.value as LlmTask)}>
                  {TASKS.map((candidate) => (
                    <option key={candidate.task} value={candidate.task}>{candidate.label}</option>
                  ))}
                </select>
              </label>
              <div className="llm-route-summary">
                {route
                  ? `${route.execution_mode} route · ${route.available ? 'available' : 'not ready'}`
                  : 'Route unavailable'}
              </div>

              <label className="llm-tool-field">
                <span>Provider override</span>
                <select
                  value={overrideProvider}
                  onChange={(e) => { setOverrideProvider(e.target.value); setOverrideModel(''); }}
                >
                  <option value="">Use route default</option>
                  <option value={LOCAL_PROVIDER}>Local (bundled)</option>
                  {descriptors.map((d) => (
                    <option key={d.provider} value={d.provider}>{d.display_name}</option>
                  ))}
                </select>
              </label>
              {overrideProvider && overrideProvider !== LOCAL_PROVIDER && (
                <label className="llm-tool-field">
                  <span>Model</span>
                  <input
                    value={overrideModel}
                    onChange={(e) => setOverrideModel(e.target.value)}
                    placeholder="model name"
                  />
                </label>
              )}

              {supportsSummaryOptions && (
                <label className="llm-tool-field">
                  <span>Summary format</span>
                  <select value={summaryVariant} onChange={(event) => setSummaryVariant(event.target.value as SummaryVariant)}>
                    <option value="plain">Plain</option>
                    <option value="mnemonic">Mnemonic</option>
                    <option value="acrostic">Acrostic</option>
                  </select>
                </label>
              )}

              {supportsStyleOptions && (
                <>
                  <label className="llm-tool-field">
                    <span>Mode</span>
                    <select value={rewriteMode} onChange={(event) => setRewriteMode(event.target.value as RewriteMode)}>
                      <option value="standard">Standard</option>
                      <option value="simplify">Simplify</option>
                    </select>
                  </label>
                  <label className="llm-tool-field">
                    <span>Style card</span>
                    <select value={styleCardId} onChange={(event) => setStyleCardId(event.target.value)}>
                      <option value="">No style card</option>
                      {styleCards.map((card) => (
                        <option key={card.id} value={card.id}>{card.name}</option>
                      ))}
                    </select>
                  </label>
                  <div className="llm-tool-overrides">
                    <span className="llm-tool-overrides-label">Style overrides</span>
                    <div className="llm-tool-overrides-grid">
                      <label className="llm-tool-field">
                        <span>Verbosity</span>
                        <select value={overrideVerbosity} onChange={(e) => setOverrideVerbosity(e.target.value as StyleVerbosity | '')}>
                          <option value="">Card default</option>
                          {VERBOSITIES.map((v) => <option key={v} value={v}>{humanize(v)}</option>)}
                        </select>
                      </label>
                      <label className="llm-tool-field">
                        <span>Perspective</span>
                        <select value={overridePerspective} onChange={(e) => setOverridePerspective(e.target.value as StylePerspective | '')}>
                          <option value="">Card default</option>
                          {PERSPECTIVES.map((p) => <option key={p} value={p}>{humanize(p)}</option>)}
                        </select>
                      </label>
                      <label className="llm-tool-field">
                        <span>Reading level</span>
                        <select value={overrideReadingLevel} onChange={(e) => setOverrideReadingLevel(e.target.value as StyleReadingLevel | '')}>
                          <option value="">Card default</option>
                          {READING_LEVELS.map((r) => <option key={r} value={r}>{humanize(r)}</option>)}
                        </select>
                      </label>
                      <label className="llm-tool-field">
                        <span>Voice</span>
                        <select value={overrideVoice} onChange={(e) => setOverrideVoice(e.target.value as StyleVoice | '')}>
                          <option value="">Card default</option>
                          {VOICES.map((v) => <option key={v} value={v}>{humanize(v)}</option>)}
                        </select>
                      </label>
                    </div>
                    <textarea
                      className="llm-tool-override-notes"
                      value={overrideNotes}
                      onChange={(e) => setOverrideNotes(e.target.value)}
                      rows={2}
                      placeholder="Additional one-run style notes…"
                    />
                  </div>
                </>
              )}

              <label className="llm-tool-checkbox">
                <input
                  type="checkbox"
                  checked
                  disabled
                  onChange={(event) => setStoreResultAsCommentary(event.target.checked)}
                />
                Review completed result as linked commentary
              </label>
            </div>
            <div className="llm-modal-actions">
              <button className="picker-btn picker-btn-secondary" onClick={() => setOpen(false)} disabled={busy}>
                Cancel
              </button>
              <button className="picker-btn picker-btn-primary" onClick={enqueue} disabled={busy || route?.available === false}>
                {busy ? 'Queueing...' : 'Queue job'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
