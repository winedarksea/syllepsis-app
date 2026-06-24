// In-app AI writing tools. Wraps the existing generate_proposal / accept_proposal commands so the
// user can run our own LLM tasks (summarize, grammar, rewrite, …) instead of relying on the OS-level
// Apple Intelligence "Writing Tools". A generated proposal is previewed before it is applied.

import { useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api';
import { Icon } from '../components/Icon';
import type { LlmTask, NoteDto, Proposal } from '../types';

const TASKS: { task: LlmTask; label: string }[] = [
  { task: 'summarize', label: 'Generate summary' },
  { task: 'grammar', label: 'Fix grammar & clarity' },
  { task: 'rewrite', label: 'Rewrite body' },
  { task: 'category_suggest', label: 'Suggest categories' },
  { task: 'fact_check', label: 'Fact-check' },
  { task: 'devils_advocate', label: "Devil's advocate" },
];

type GenerationPhase = 'checking' | 'local' | 'cloud';

interface GenerationProgress {
  task: LlmTask;
  label: string;
  phase: GenerationPhase;
  provider?: string;
  model?: string;
  startedAt: number;
}

function waitForNextPaint(): Promise<void> {
  return new Promise((resolve) => {
    window.requestAnimationFrame(() => resolve());
  });
}

function proposalModelLabel(proposal: Proposal): string {
  const executionMode = proposal.provider === 'local' ? 'local LLM' : 'model';
  return `${proposal.provider}/${proposal.model} (${executionMode})`;
}

interface Props {
  noteId: string;
  /** Called with the updated note after a proposal is accepted, so the editor can reload. */
  onApplied: (updated: NoteDto) => void;
}

export function LlmToolsMenu({ noteId, onApplied }: Props) {
  const [open, setOpen] = useState(false);
  const [busyTask, setBusyTask] = useState<LlmTask | null>(null);
  const [progress, setProgress] = useState<GenerationProgress | null>(null);
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const [proposal, setProposal] = useState<Proposal | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!progress) {
      setElapsedSeconds(0);
      return;
    }
    const updateElapsed = () => {
      setElapsedSeconds(Math.max(0, Math.floor((Date.now() - progress.startedAt) / 1000)));
    };
    updateElapsed();
    const timer = window.setInterval(updateElapsed, 1000);
    return () => window.clearInterval(timer);
  }, [progress]);

  const generate = useCallback(async (task: LlmTask) => {
    setOpen(false);
    setBusyTask(task);
    setError(null);
    setProposal(null);
    const label = TASKS.find((candidate) => candidate.task === task)?.label ?? task;
    const startedAt = Date.now();
    setProgress({ task, label, phase: 'checking', startedAt });
    try {
      const route = (await api.llmRouteStatuses()).find((candidate) => candidate.task === task);
      if (!route) {
        throw new Error(`No LLM route is configured for ${task}.`);
      }
      if (!route.available) {
        if (route.execution_mode === 'disabled') {
          throw new Error('LLM features are disabled for this book.');
        }
        throw new Error(
          `No runnable LLM is configured for ${task}. Configure ${route.provider}/${route.model} or choose a cloud/server route.`,
        );
      }
      if (route.execution_mode === 'cloud') {
        setProgress({
          task,
          label,
          phase: 'cloud',
          provider: route.provider,
          model: route.model,
          startedAt,
        });
        setProposal(await api.generateCloudProposal(noteId, task));
      } else if (route.execution_mode === 'local') {
        setProgress({
          task,
          label,
          phase: 'local',
          provider: route.provider,
          model: route.model,
          startedAt,
        });
        // Let the native WebView paint the progress overlay before starting expensive inference.
        await waitForNextPaint();
        setProposal(await api.generateProposal(noteId, task));
      } else {
        throw new Error(`No runnable LLM is configured for ${task}.`);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyTask(null);
      setProgress(null);
    }
  }, [noteId]);

  const accept = useCallback(async () => {
    if (!proposal) return;
    try {
      const updated = await api.acceptProposal(proposal);
      setProposal(null);
      onApplied(updated);
    } catch (e) {
      setError(String(e));
    }
  }, [proposal, onApplied]);

  const busyLabel = busyTask ? TASKS.find((t) => t.task === busyTask)?.label : null;
  const progressTitle = progress
    ? progress.phase === 'checking'
      ? 'Checking LLM route'
      : progress.phase === 'local'
        ? 'Running local LLM'
        : 'Contacting LLM provider'
    : '';
  const progressDetail = progress
    ? progress.phase === 'checking'
      ? progress.label
      : `${progress.provider}/${progress.model}`
    : '';

  return (
    <div className="llm-tools">
      <button
        className="llm-tools-btn"
        onClick={() => setOpen((v) => !v)}
        disabled={busyTask !== null}
        title="AI writing tools"
      >
        <Icon name="auto_awesome" size={16} />
        <span>{busyLabel ? `${busyLabel}…` : 'Tools'}</span>
      </button>

      {open && (
        <div className="llm-tools-menu">
          {TASKS.map((t) => (
            <button key={t.task} className="llm-tools-item" onClick={() => generate(t.task)}>
              {t.label}
            </button>
          ))}
        </div>
      )}

      {error && <div className="llm-tools-error" onClick={() => setError(null)}>{error}</div>}

      {progress && (
        <div className="llm-progress-backdrop" role="presentation">
          <div className="llm-progress" role="status" aria-live="polite">
            <div className="llm-progress-spinner" aria-hidden="true" />
            <div className="llm-progress-copy">
              <div className="llm-progress-title">{progressTitle}</div>
              <div className="llm-progress-detail">{progressDetail}</div>
              <div className="llm-progress-time">{elapsedSeconds}s elapsed</div>
            </div>
          </div>
        </div>
      )}

      {proposal && (
        <div className="llm-modal-backdrop" role="presentation" onClick={() => setProposal(null)}>
          <div className="llm-modal" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
            <div className="llm-modal-header">
              <h3>{TASKS.find((t) => t.task === proposal.task)?.label ?? proposal.task}</h3>
              <span className="llm-modal-model">
                {proposalModelLabel(proposal)}
              </span>
            </div>
            <pre className="llm-modal-content">{proposal.content}</pre>
            <div className="llm-modal-actions">
              <button className="picker-btn picker-btn-secondary" onClick={() => setProposal(null)}>
                Discard
              </button>
              <button className="picker-btn picker-btn-primary" onClick={accept}>
                Apply
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
