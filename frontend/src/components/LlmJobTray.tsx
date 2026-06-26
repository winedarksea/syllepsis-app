import { useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import type { QueuedLlmJobResult } from '../types';
import { Icon } from './Icon';

function taskLabel(task: string): string {
  return task.replaceAll('_', ' ');
}

function relativeTime(jobId: string): string {
  // Job IDs are ULIDs: first 10 chars encode a 48-bit ms timestamp
  try {
    const ms = parseInt(jobId.slice(0, 10), 32);
    const seconds = Math.max(0, Math.round((Date.now() - ms) / 1000));
    if (seconds < 60) return 'just now';
    const minutes = Math.round(seconds / 60);
    if (minutes < 60) return `${minutes}m ago`;
    const hours = Math.round(minutes / 60);
    return hours < 48 ? `${hours}h ago` : `${Math.round(hours / 24)}d ago`;
  } catch {
    return '';
  }
}

function JobCard({
  job,
  onAccept,
  onDismiss,
  onOpen,
}: {
  job: QueuedLlmJobResult;
  onAccept?: (job: QueuedLlmJobResult) => void;
  onDismiss?: (jobId: string) => void;
  onOpen: (noteId: string) => void;
}) {
  return (
    <div
      className={`llm-job-card ${job.status}`}
      onClick={() => onOpen(job.target_note_id)}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') onOpen(job.target_note_id); }}
    >
      <div className="llm-job-main">
        <span className="llm-job-title">{taskLabel(job.task)}</span>
        <span className="llm-job-status-badge">{job.status}</span>
        <span className="llm-job-time">{relativeTime(job.job_id)}</span>
        {job.error && <span className="llm-job-error">{job.error}</span>}
      </div>
      <div className="llm-job-actions" onClick={(e) => e.stopPropagation()}>
        <button title="Open target note" onClick={() => onOpen(job.target_note_id)}>
          <Icon name="open_in_new" size={14} />
        </button>
        {job.status === 'complete' && job.proposal && onAccept && (
          <button className="llm-job-apply-btn" onClick={() => onAccept(job)}>Apply</button>
        )}
        {(job.status === 'complete' || job.status === 'failed') && onDismiss && (
          <button title="Dismiss" onClick={() => onDismiss(job.job_id)}>
            <Icon name="close" size={14} />
          </button>
        )}
      </div>
    </div>
  );
}

function LlmJobHistory({ onClose }: { onClose: () => void }) {
  const { openEditor } = useStore();
  const [jobs, setJobs] = useState<QueuedLlmJobResult[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.listAllLlmJobs().then(setJobs).catch((e) => setError(String(e)));
    const timer = window.setInterval(() => {
      api.listAllLlmJobs().then(setJobs).catch(() => {});
    }, 3000);
    return () => window.clearInterval(timer);
  }, []);

  const accept = useCallback(async (job: QueuedLlmJobResult) => {
    try {
      await api.acceptLlmJobResult(job.job_id);
      openEditor(job.target_note_id, 'read');
      api.listAllLlmJobs().then(setJobs).catch(() => {});
    } catch (e) {
      setError(String(e));
    }
  }, [openEditor]);

  const dismiss = useCallback(async (jobId: string) => {
    try {
      await api.dismissLlmJobResult(jobId);
      api.listAllLlmJobs().then(setJobs).catch(() => {});
    } catch (e) {
      setError(String(e));
    }
  }, []);

  return (
    <div className="llm-job-history-panel">
      <div className="llm-job-history-header">
        <span>Job history</span>
        <button onClick={onClose} title="Close"><Icon name="close" size={14} /></button>
      </div>
      {error && <div className="llm-job-card error" onClick={() => setError(null)}>{error}</div>}
      {jobs.length === 0 && <div className="llm-job-history-empty">No jobs yet.</div>}
      {jobs.map((job) => (
        <JobCard
          key={job.job_id}
          job={job}
          onAccept={job.status === 'complete' && job.proposal ? accept : undefined}
          onDismiss={job.status !== 'running' && job.status !== 'queued' ? dismiss : undefined}
          onOpen={(id) => openEditor(id, 'read')}
        />
      ))}
    </div>
  );
}

export function LlmJobTray() {
  const { openEditor, bumpNoteReload } = useStore();
  const [jobs, setJobs] = useState<QueuedLlmJobResult[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [historyOpen, setHistoryOpen] = useState(false);

  const refresh = useCallback(() => {
    api.listLlmJobs().then(setJobs).catch((e) => setError(String(e)));
  }, []);

  useEffect(() => {
    refresh();
    const timer = window.setInterval(refresh, 2500);
    return () => window.clearInterval(timer);
  }, [refresh]);

  const accept = useCallback(async (job: QueuedLlmJobResult) => {
    try {
      const updated = await api.acceptLlmJobResult(job.job_id);
      await api.dismissLlmJobResult(job.job_id);
      openEditor(updated.id, 'read');
      bumpNoteReload();
      refresh();
    } catch (e) {
      setError(String(e));
    }
  }, [bumpNoteReload, openEditor, refresh]);

  const dismiss = useCallback(async (jobId: string) => {
    try {
      await api.dismissLlmJobResult(jobId);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  }, [refresh]);

  const visibleJobs = jobs.filter((job) => job.status !== 'complete' || job.proposal);
  if (visibleJobs.length === 0 && !error && !historyOpen) return null;

  return (
    <>
      {historyOpen && <LlmJobHistory onClose={() => setHistoryOpen(false)} />}
      <div className="llm-job-tray" aria-live="polite">
        {error && (
          <div className="llm-job-card error" onClick={() => setError(null)}>{error}</div>
        )}
        {visibleJobs.slice(-4).map((job) => (
          <JobCard
            key={job.job_id}
            job={job}
            onAccept={accept}
            onDismiss={dismiss}
            onOpen={(id) => openEditor(id, 'read')}
          />
        ))}
        <div className="llm-job-tray-footer">
          <button
            className="llm-job-history-btn"
            onClick={() => setHistoryOpen((v) => !v)}
            title="Job history"
          >
            <Icon name="history" size={14} />
            History
          </button>
        </div>
      </div>
    </>
  );
}
