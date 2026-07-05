// Trash — soft-deleted notes awaiting permanent purge. Deleted notes wait here for the
// configured deletion delay before being removed; each can be restored until then. Sweep now
// purges everything already past its purge date; Delete immediately purges the whole trash.

import { useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import { PageHeader } from '../components/PageHeader';
import type { PendingDeletion } from '../types';
import './TrashView.css';

function daysUntil(iso: string): string {
  const ms = new Date(iso).getTime() - Date.now();
  const days = Math.ceil(ms / 86_400_000);
  if (days <= 0) return 'due now';
  return days === 1 ? 'in 1 day' : `in ${days} days`;
}

export function TrashView() {
  const { openEditor } = useStore();
  const [pending, setPending] = useState<PendingDeletion[] | null>(null);
  const [delayDays, setDelayDays] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirmDeleteAll, setConfirmDeleteAll] = useState(false);

  const load = useCallback(() => {
    api.policyOverview().then((p) => setPending(p.pending_deletion)).catch((e) => setError(String(e)));
    api.getBookConfig().then((c) => setDelayDays(c.cleanup.deletion_delay_days)).catch(() => undefined);
  }, []);

  useEffect(() => { load(); }, [load]);

  const act = useCallback(async (fn: () => Promise<unknown>, message?: string) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
      if (message) setNotice(message);
      load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, [load]);

  if (error && !pending) return <div className="tv-state tv-error">{error}</div>;
  if (!pending) return <div className="tv-state">Loading trash…</div>;

  return (
    <div className="tv-root">
      <PageHeader title="Trash">
        <button className="tv-btn" disabled={busy || pending.length === 0}
          onClick={() => act(async () => { const ids = await api.purgeExpired(); setNotice(`Swept ${ids.length} expired note${ids.length !== 1 ? 's' : ''}.`); })}>
          Sweep now
        </button>
        {confirmDeleteAll ? (
          <span className="tv-confirm-inline">
            Delete all {pending.length} now?{' '}
            <button className="tv-btn tv-btn--danger" disabled={busy}
              onClick={() => act(async () => { const ids = await api.purgeAllTrash(); setConfirmDeleteAll(false); setNotice(`Permanently deleted ${ids.length} note${ids.length !== 1 ? 's' : ''}.`); })}>
              Confirm
            </button>
            {' '}
            <button className="tv-btn" disabled={busy} onClick={() => setConfirmDeleteAll(false)}>Cancel</button>
          </span>
        ) : (
          <button className="tv-btn tv-btn--danger" disabled={busy || pending.length === 0} onClick={() => setConfirmDeleteAll(true)}>
            Delete immediately
          </button>
        )}
      </PageHeader>

      {notice && <div className="tv-notice" onClick={() => setNotice(null)}>{notice}</div>}
      {error && <div className="tv-state tv-error">{error}</div>}

      {pending.length === 0 ? (
        <div className="tv-state tv-empty">
          <p>Trash is empty.</p>
          <p>
            Deleted notes wait here
            {delayDays != null ? ` for ${delayDays} day${delayDays !== 1 ? 's' : ''}` : ''}
            {' '}before being removed.
          </p>
        </div>
      ) : (
        <section className="tv-list">
          <p className="tv-hint">Marked for deletion; permanently removed after the delay. Restore to cancel.</p>
          {pending.map((p) => (
            <div key={p.id} className="tv-row">
              <button className="tv-name" onClick={() => openEditor(p.id)}>{p.title || '(untitled)'}</button>
              <span className="tv-meta">purges {daysUntil(p.purge_at)}</span>
              <button className="tv-undo" disabled={busy} onClick={() => act(() => api.restoreNote(p.id), 'Restored.')}>Restore</button>
            </div>
          ))}
        </section>
      )}
    </div>
  );
}
