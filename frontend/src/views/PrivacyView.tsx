// Centralized privacy & lifecycle policy panel (Phase 6, privacy-security.md). One place to see
// and reverse every restriction in the book — private/archived/locked notes, private categories,
// and the deletion-delay trash — plus the publish actions that exclude private content.

import { useCallback, useEffect, useState } from 'react';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import type { PolicyOverview } from '../types';
import './PrivacyView.css';

function daysUntil(iso: string): string {
  const ms = new Date(iso).getTime() - Date.now();
  const days = Math.ceil(ms / 86_400_000);
  if (days <= 0) return 'due now';
  return days === 1 ? 'in 1 day' : `in ${days} days`;
}

export function PrivacyView() {
  const { openEditor } = useStore();
  const [policy, setPolicy] = useState<PolicyOverview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    api.policyOverview().then(setPolicy).catch((e) => setError(String(e)));
  }, []);

  useEffect(() => { load(); }, [load]);

  // Run a mutating action, then refresh the overview and surface any error.
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

  const publish = useCallback(async () => {
    const dir = await openDialog({ directory: true, multiple: false, title: 'Choose output folder for the published site' });
    if (!dir || typeof dir !== 'string') return;
    await act(async () => {
      const report = await api.publishSite(dir);
      setNotice(`Published ${report.published_notes} note${report.published_notes !== 1 ? 's' : ''} (${report.excluded_private} private withheld) → ${report.index_path}`);
    });
  }, [act]);

  const refreshGitignore = useCallback(() => act(async () => {
    const report = await api.refreshPrivateGitignore();
    setNotice(`${report.excluded_paths.length} private path${report.excluded_paths.length !== 1 ? 's' : ''} excluded from git publish.`);
  }), [act]);

  if (error && !policy) return <div className="pv-state pv-error">{error}</div>;
  if (!policy) return <div className="pv-state">Loading policy…</div>;

  const nothing =
    policy.private_notes.length === 0 && policy.archived_notes.length === 0 &&
    policy.locked_notes.length === 0 && policy.pending_deletion.length === 0 &&
    policy.private_categories.length === 0;

  return (
    <div className="pv-root">
      <div className="pv-header">
        <h2 className="pv-title">Privacy &amp; Policy</h2>
        <div className="pv-actions">
          <button className="pv-btn" disabled={busy} onClick={publish}>Publish read-only site…</button>
          <button className="pv-btn" disabled={busy} onClick={refreshGitignore}>Refresh git exclusions</button>
        </div>
      </div>

      {notice && <div className="pv-notice" onClick={() => setNotice(null)}>{notice}</div>}
      {error && <div className="pv-state pv-error">{error}</div>}

      {nothing && <div className="pv-state">Nothing is restricted. Notes are public, unlocked, and active.</div>}

      {policy.pending_deletion.length > 0 && (
        <section className="pv-section">
          <div className="pv-section-head">
            <h3 className="pv-section-title">Trash · pending deletion ({policy.pending_deletion.length})</h3>
            <button className="pv-link-btn" disabled={busy}
              onClick={() => act(async () => { const ids = await api.purgeExpired(); setNotice(`Purged ${ids.length} expired note${ids.length !== 1 ? 's' : ''}.`); })}>
              Empty trash now
            </button>
          </div>
          <p className="pv-hint">Marked for deletion; permanently removed after the delay. Restore to cancel.</p>
          {policy.pending_deletion.map((p) => (
            <div key={p.id} className="pv-row">
              <button className="pv-name" onClick={() => openEditor(p.id)}>{p.title || '(untitled)'}</button>
              <span className="pv-meta">purges {daysUntil(p.purge_at)}</span>
              <button className="pv-undo" disabled={busy} onClick={() => act(() => api.restoreNote(p.id), 'Restored.')}>Restore</button>
            </div>
          ))}
        </section>
      )}

      {policy.private_notes.length > 0 && (
        <section className="pv-section">
          <h3 className="pv-section-title">Private notes ({policy.private_notes.length})</h3>
          <p className="pv-hint">Hidden from default views, search/RAG, and the publish.</p>
          {policy.private_notes.map((n) => (
            <div key={n.id} className="pv-row">
              <button className="pv-name" onClick={() => openEditor(n.id)}>{n.title || '(untitled)'}</button>
              <button className="pv-undo" disabled={busy} onClick={() => act(() => api.setNotePrivate(n.id, false), 'Made public.')}>Make public</button>
            </div>
          ))}
        </section>
      )}

      {policy.locked_notes.length > 0 && (
        <section className="pv-section">
          <h3 className="pv-section-title">Locked notes ({policy.locked_notes.length})</h3>
          <p className="pv-hint">Body edits go through an unlock delay or a fact-check gate before merging.</p>
          {policy.locked_notes.map((n) => (
            <div key={n.id} className="pv-row">
              <button className="pv-name" onClick={() => openEditor(n.id)}>{n.title || '(untitled)'}</button>
              <span className="pv-tag">{n.mode === 'unlock_delay' ? 'unlock delay' : 'fact-check gate'}</span>
              <button className="pv-undo" disabled={busy} onClick={() => act(() => api.setNoteLock(n.id, 'none'), 'Unlocked.')}>Unlock</button>
            </div>
          ))}
        </section>
      )}

      {policy.archived_notes.length > 0 && (
        <section className="pv-section">
          <h3 className="pv-section-title">Archived notes ({policy.archived_notes.length})</h3>
          <p className="pv-hint">Kept but hidden from default views; reversible.</p>
          {policy.archived_notes.map((n) => (
            <div key={n.id} className="pv-row">
              <button className="pv-name" onClick={() => openEditor(n.id)}>{n.title || '(untitled)'}</button>
              <button className="pv-undo" disabled={busy} onClick={() => act(() => api.setNoteArchived(n.id, false), 'Unarchived.')}>Unarchive</button>
            </div>
          ))}
        </section>
      )}

      {policy.private_categories.length > 0 && (
        <section className="pv-section">
          <h3 className="pv-section-title">Private categories ({policy.private_categories.length})</h3>
          <p className="pv-hint">Their notes are excluded from the publish and default views.</p>
          {policy.private_categories.map((name) => (
            <div key={name} className="pv-row">
              <span className="pv-name pv-name-static">#{name}</span>
              <button className="pv-undo" disabled={busy} onClick={() => act(() => api.setCategoryPrivate(name, false), 'Made public.')}>Make public</button>
            </div>
          ))}
        </section>
      )}
    </div>
  );
}
