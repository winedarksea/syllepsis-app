// Queued AI writing tools. The editor only enqueues work; completed results are reviewed from the
// app-level job tray so the user can navigate away while local/cloud inference runs.

import { useCallback, useEffect, useMemo, useState } from 'react';
import { api } from '../lib/api';
import { Icon } from '../components/Icon';
import type {
  LlmRouteStatus, LlmTask, ModelRef, QueuedLlmJobResult, RewriteMode, StyleCard, SummaryVariant,
} from '../types';

const TASKS: { task: LlmTask; label: string }[] = [
  { task: 'summarize', label: 'Generate summary' },
  { task: 'generate_from_summary', label: 'Generate from summary' },
  { task: 'grammar', label: 'Fix grammar & clarity' },
  { task: 'rewrite', label: 'Rewrite body' },
  { task: 'category_suggest', label: 'Suggest categories' },
  { task: 'fact_check', label: 'Fact-check' },
  { task: 'devils_advocate', label: "Devil's advocate" },
];

interface Props {
  noteId: string;
  onQueued?: (job: QueuedLlmJobResult) => void;
}

export function LlmToolsMenu({ noteId, onQueued }: Props) {
  const [open, setOpen] = useState(false);
  const [routes, setRoutes] = useState<LlmRouteStatus[]>([]);
  const [styleCards, setStyleCards] = useState<StyleCard[]>([]);
  const [task, setTask] = useState<LlmTask>('summarize');
  const [modelOverride, setModelOverride] = useState<ModelRef | null>(null);
  const [summaryVariant, setSummaryVariant] = useState<SummaryVariant>('plain');
  const [rewriteMode, setRewriteMode] = useState<RewriteMode>('standard');
  const [styleCardId, setStyleCardId] = useState('');
  const [styleOverrides, setStyleOverrides] = useState('');
  const [storeResultAsCommentary, setStoreResultAsCommentary] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    api.llmRouteStatuses().then(setRoutes).catch((e) => setError(String(e)));
    api.listStyleCards().then(setStyleCards).catch(() => setStyleCards([]));
  }, [open]);

  const route = useMemo(
    () => routes.find((candidate) => candidate.task === task) ?? null,
    [routes, task],
  );
  const selectedModel = modelOverride ?? (route ? { provider: route.provider, model: route.model } : null);
  const supportsSummaryOptions = task === 'summarize';
  const supportsStyleOptions = task === 'rewrite' || task === 'generate_from_summary';

  const enqueue = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const job = await api.enqueueLlmJob({
        target_note_id: noteId,
        task,
        model_override: modelOverride,
        style_card_id: supportsStyleOptions && styleCardId ? styleCardId : null,
        style_overrides: supportsStyleOptions && styleOverrides.trim() ? styleOverrides.trim() : null,
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
    styleOverrides,
    summaryVariant,
    supportsStyleOptions,
    supportsSummaryOptions,
    task,
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
              {selectedModel && (
                <span className="llm-modal-model">{selectedModel.provider}/{selectedModel.model}</span>
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
                <span>Provider/model override</span>
                <input
                  value={modelOverride ? `${modelOverride.provider}/${modelOverride.model}` : ''}
                  onChange={(event) => {
                    const value = event.target.value.trim();
                    if (!value) { setModelOverride(null); return; }
                    const [provider, ...modelParts] = value.split('/');
                    setModelOverride({ provider, model: modelParts.join('/') });
                  }}
                  placeholder={route ? `${route.provider}/${route.model}` : 'provider/model'}
                />
              </label>

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
                  <label className="llm-tool-field">
                    <span>Style overrides</span>
                    <textarea
                      value={styleOverrides}
                      onChange={(event) => setStyleOverrides(event.target.value)}
                      rows={3}
                      placeholder="Optional one-run style instructions"
                    />
                  </label>
                </>
              )}

              <label className="llm-tool-checkbox">
                <input
                  type="checkbox"
                  checked={storeResultAsCommentary}
                  onChange={(event) => setStoreResultAsCommentary(event.target.checked)}
                />
                Save completed result as linked commentary
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
