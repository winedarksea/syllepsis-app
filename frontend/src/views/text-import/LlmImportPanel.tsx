import type { ImportLlmJobResult, LlmRouteStatus, TextImportLlmChunk } from '../../types';
import { Icon } from '../../components/Icon';
import { Toggle } from './ImportControls';
import type { EditableImportItem } from './PreviewItemCard';

export type ChunkSide = 'deterministic' | 'ai';

const EXECUTION_LABEL: Record<string, string> = {
  disabled: 'AI features are disabled for this book',
  local: 'local model',
  cloud: 'cloud model',
  unavailable: 'no runnable model configured',
};

interface LlmImportPanelProps {
  enabled: boolean;
  onEnabledChange: (enabled: boolean) => void;
  routeStatus: LlmRouteStatus | undefined;
  promptOverride: string;
  onPromptOverrideChange: (value: string) => void;
  chunkMaxChars: number;
  onChunkMaxCharsChange: (value: number) => void;
  chunks: TextImportLlmChunk[] | null;
  jobs: Record<number, ImportLlmJobResult>;
  chunkSides: Record<number, ChunkSide>;
  proposalChecks: Record<number, boolean[]>;
  running: boolean;
  progress: string | null;
  items: EditableImportItem[];
  onRunAll: () => void;
  onRunChunk: (chunkIndex: number) => void;
  onCancel: () => void;
  onSideChange: (chunkIndex: number, side: ChunkSide) => void;
  onProposalCheckChange: (chunkIndex: number, proposalIndex: number, checked: boolean) => void;
}

/**
 * AI-assisted chunk splitting: chunks run sequentially through the import_split queue; each
 * completed chunk gets a side-by-side review where the user picks which side commits.
 */
export function LlmImportPanel(props: LlmImportPanelProps) {
  const {
    enabled, onEnabledChange, routeStatus,
    promptOverride, onPromptOverrideChange,
    chunkMaxChars, onChunkMaxCharsChange,
    chunks, jobs, chunkSides, proposalChecks,
    running, progress, items,
    onRunAll, onRunChunk, onCancel, onSideChange, onProposalCheckChange,
  } = props;

  const routeAvailable = routeStatus?.available ?? false;

  return (
    <section className="ti-subpanel">
      <div className="ti-subpanel-header">
        <h4>AI-assisted splitting</h4>
        <Toggle label="Enable" checked={enabled} onChange={onEnabledChange} />
      </div>

      {enabled && (
        <>
          <p className="ti-subtitle">
            {routeStatus
              ? <>Routes to <strong>{routeStatus.provider}/{routeStatus.model}</strong> ({EXECUTION_LABEL[routeStatus.execution_mode] ?? routeStatus.execution_mode}).</>
              : 'Loading model route…'}
            {routeStatus && !routeAvailable && ' Configure a model in Settings to run AI splitting.'}
          </p>

          <label className="ti-field">
            <span>Extra instructions (optional, saved on this device)</span>
            <textarea
              className="ti-override-input"
              value={promptOverride}
              onChange={(event) => onPromptOverrideChange(event.target.value)}
              placeholder="e.g. Keep every measurement; prefer notes under 6 lines."
            />
          </label>

          <div className="ti-field-row ti-llm-run-row">
            <label className="ti-field ti-chunk-size-field" title="Larger chunks give the model more context but risk truncated replies.">
              <span>Chunk size (chars)</span>
              <input
                type="number"
                min={500}
                step={500}
                value={chunkMaxChars}
                onChange={(event) => {
                  const next = Number(event.target.value);
                  if (Number.isFinite(next)) onChunkMaxCharsChange(Math.max(500, Math.floor(next)));
                }}
              />
            </label>
            {running ? (
              <button className="ti-btn" onClick={onCancel}>
                <Icon name="stop_circle" size={18} />
                <span>{progress ? `Cancel (${progress})` : 'Cancel'}</span>
              </button>
            ) : (
              <button className="ti-btn ti-btn-primary" disabled={!routeAvailable || !chunks || chunks.length === 0} onClick={onRunAll}>
                <Icon name="auto_awesome" size={18} />
                <span>Run AI split{chunks ? ` (${chunks.length} chunk${chunks.length === 1 ? '' : 's'})` : ''}</span>
              </button>
            )}
          </div>
          {running && (
            <p className="ti-subtitle">A running chunk cannot be interrupted; cancel stops before the next chunk.</p>
          )}

          {chunks?.map((chunk) => (
            <ChunkReview
              key={chunk.index}
              chunk={chunk}
              job={jobs[chunk.index]}
              side={chunkSides[chunk.index] ?? 'deterministic'}
              checks={proposalChecks[chunk.index]}
              running={running}
              routeAvailable={routeAvailable}
              items={items}
              onRun={() => onRunChunk(chunk.index)}
              onSideChange={(side) => onSideChange(chunk.index, side)}
              onCheckChange={(proposalIndex, checked) => onProposalCheckChange(chunk.index, proposalIndex, checked)}
            />
          ))}
        </>
      )}
    </section>
  );
}

function ChunkReview({ chunk, job, side, checks, running, routeAvailable, items, onRun, onSideChange, onCheckChange }: {
  chunk: TextImportLlmChunk;
  job: ImportLlmJobResult | undefined;
  side: ChunkSide;
  checks: boolean[] | undefined;
  running: boolean;
  routeAvailable: boolean;
  items: EditableImportItem[];
  onRun: () => void;
  onSideChange: (side: ChunkSide) => void;
  onCheckChange: (proposalIndex: number, checked: boolean) => void;
}) {
  const deterministicItems = chunk.item_indices
    .map((position) => items[position])
    .filter((item): item is EditableImportItem => Boolean(item));
  const complete = job?.status === 'complete';
  const parsed = complete && (job?.proposed.length ?? 0) > 0;
  const statusLabel = !job
    ? 'not run'
    : job.status === 'complete'
      ? (parsed ? `${job.proposed.length} proposed` : 'reply did not parse')
      : job.status;

  return (
    <div className="ti-chunk">
      <div className="ti-chunk-header">
        <span className="ti-chip">{chunk.heading ? `#${chunk.heading}` : 'unsectioned'}</span>
        <span className="ti-suggestion-meta">
          chunk {chunk.index + 1} · {deterministicItems.length} item{deterministicItems.length === 1 ? '' : 's'} · {statusLabel}
        </span>
        <span className="ti-item-header-spacer" />
        <button className="ti-btn ti-btn-small" disabled={running || !routeAvailable} onClick={onRun}>
          {job ? 'Re-run' : 'Run'}
        </button>
      </div>

      {chunk.warnings.map((warning) => (
        <div key={warning} className="ti-warning">{warning}</div>
      ))}
      {job?.status === 'failed' && job.error && <div className="ti-error">{job.error}</div>}

      {complete && (
        <>
          {!parsed && job?.error && (
            <div className="ti-warning">AI reply could not be parsed ({job.error}); the deterministic split is kept.</div>
          )}
          <div className="ti-chunk-compare">
            <div className={`ti-chunk-side${side === 'deterministic' ? ' ti-chunk-side-chosen' : ''}`}>
              <label className="ti-chunk-side-pick">
                <input
                  type="radio"
                  name={`ti-chunk-side-${chunk.index}`}
                  checked={side === 'deterministic'}
                  onChange={() => onSideChange('deterministic')}
                />
                <span>Deterministic split ({deterministicItems.length})</span>
              </label>
              <ul className="ti-chunk-note-list">
                {deterministicItems.map((item) => (
                  <li key={item.index} className={item.accepted ? '' : 'ti-item-rejected'}>
                    <strong>{item.title || 'Untitled'}</strong>
                  </li>
                ))}
              </ul>
              <p className="ti-subtitle">Items stay editable in the preview list above.</p>
            </div>
            <div className={`ti-chunk-side${side === 'ai' ? ' ti-chunk-side-chosen' : ''}`}>
              <label className="ti-chunk-side-pick">
                <input
                  type="radio"
                  name={`ti-chunk-side-${chunk.index}`}
                  checked={side === 'ai'}
                  disabled={!parsed}
                  onChange={() => onSideChange('ai')}
                />
                <span>AI proposal ({job?.proposed.length ?? 0})</span>
              </label>
              <ul className="ti-chunk-note-list">
                {job?.proposed.map((proposal, proposalIndex) => (
                  <li key={proposalIndex}>
                    <label className="ti-proposal">
                      <input
                        type="checkbox"
                        checked={checks?.[proposalIndex] ?? true}
                        disabled={side !== 'ai'}
                        onChange={(event) => onCheckChange(proposalIndex, event.target.checked)}
                      />
                      <span>
                        <strong>{proposal.title || 'Untitled'}</strong>
                        {proposal.categories.length > 0 && (
                          <span className="ti-suggestion-meta"> {proposal.categories.map((c) => `#${c}`).join(' ')}</span>
                        )}
                        <span className="ti-proposal-body">{proposal.body}</span>
                      </span>
                    </label>
                  </li>
                ))}
              </ul>
            </div>
          </div>
        </>
      )}

      {job?.raw_output && (!parsed || job.error) && (
        <details className="ti-raw-output">
          <summary>Raw model output</summary>
          <pre>{job.raw_output}</pre>
        </details>
      )}
    </div>
  );
}
