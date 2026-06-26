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
  onDismiss,
  onOpen,
}: {
  job: QueuedLlmJobResult;
  onDismiss?: (jobId: string) => void;
  onOpen: (job: QueuedLlmJobResult) => void;
}) {
  return (
    <div
      className={`llm-job-card ${job.status}`}
      onClick={() => onOpen(job)}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') onOpen(job); }}
    >
      <div className="llm-job-main">
        <span className="llm-job-title">{taskLabel(job.task)}</span>
        <span className="llm-job-status-badge">{job.status}</span>
        <span className="llm-job-time">{relativeTime(job.job_id)}</span>
        {job.error && <span className="llm-job-error">{job.error}</span>}
      </div>
      <div className="llm-job-actions" onClick={(e) => e.stopPropagation()}>
        <button title="Open result" onClick={() => onOpen(job)}>
          <Icon name="open_in_new" size={14} />
        </button>
        {job.status === 'complete' && job.commentary_id && (
          <button className="llm-job-apply-btn" onClick={() => onOpen(job)}>Review</button>
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
  const { openEditor, openCommentary } = useStore();
  const [jobs, setJobs] = useState<QueuedLlmJobResult[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.listAllLlmJobs().then(setJobs).catch((e) => setError(String(e)));
    const timer = window.setInterval(() => {
      api.listAllLlmJobs().then(setJobs).catch(() => {});
    }, 3000);
    return () => window.clearInterval(timer);
  }, []);

  const dismiss = useCallback(async (jobId: string) => {
    try {
      await api.dismissLlmJobResult(jobId);
      api.listAllLlmJobs().then(setJobs).catch(() => {});
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const openJob = useCallback((job: QueuedLlmJobResult) => {
    if (job.commentary_id) openCommentary(job.target_note_id, job.commentary_id);
    else openEditor(job.target_note_id, 'read');
  }, [openCommentary, openEditor]);

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
          onDismiss={job.status !== 'running' && job.status !== 'queued' ? dismiss : undefined}
          onOpen={openJob}
        />
      ))}
    </div>
  );
}

export function LlmJobTray() {
  const { openEditor, openCommentary } = useStore();
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

  const dismiss = useCallback(async (jobId: string) => {
    try {
      await api.dismissLlmJobResult(jobId);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  }, [refresh]);

  const openJob = useCallback((job: QueuedLlmJobResult) => {
    if (job.commentary_id) openCommentary(job.target_note_id, job.commentary_id);
    else openEditor(job.target_note_id, 'read');
  }, [openCommentary, openEditor]);

  const visibleJobs = jobs.filter((job) => job.status !== 'complete' || job.commentary_id);
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
            onDismiss={dismiss}
            onOpen={openJob}
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
