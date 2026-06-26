import { useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import type { QueuedLlmJobResult } from '../types';
import { Icon } from './Icon';

function taskLabel(task: string): string {
  return task.replaceAll('_', ' ');
}

export function LlmJobTray() {
  const { openEditor } = useStore();
  const [jobs, setJobs] = useState<QueuedLlmJobResult[]>([]);
  const [error, setError] = useState<string | null>(null);

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
      refresh();
    } catch (e) {
      setError(String(e));
    }
  }, [openEditor, refresh]);

  const dismiss = useCallback(async (jobId: string) => {
    try {
      await api.dismissLlmJobResult(jobId);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  }, [refresh]);

  const visibleJobs = jobs.filter((job) => job.status !== 'complete' || job.proposal);
  if (visibleJobs.length === 0 && !error) return null;

  return (
    <div className="llm-job-tray" aria-live="polite">
      {error && (
        <div className="llm-job-card error" onClick={() => setError(null)}>
          {error}
        </div>
      )}
      {visibleJobs.slice(-4).map((job) => (
        <div key={job.job_id} className={`llm-job-card ${job.status}`}>
          <div className="llm-job-main">
            <span className="llm-job-title">{taskLabel(job.task)}</span>
            <span className="llm-job-status">{job.status}</span>
            {job.error && <span className="llm-job-error">{job.error}</span>}
          </div>
          <div className="llm-job-actions">
            <button title="Open target note" onClick={() => openEditor(job.target_note_id, 'read')}>
              <Icon name="open_in_new" size={14} />
            </button>
            {job.status === 'complete' && job.proposal && (
              <button onClick={() => accept(job)}>Apply</button>
            )}
            {(job.status === 'complete' || job.status === 'failed') && (
              <button title="Dismiss" onClick={() => dismiss(job.job_id)}>
                <Icon name="close" size={14} />
              </button>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}
