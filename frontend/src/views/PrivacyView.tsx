// Centralized privacy & lifecycle policy panel (Phase 6, privacy-security.md). One place to see
// and reverse every restriction in the book. Privacy is three independent capabilities — hidden,
// excluded-from-search, and excluded-from-publish — each listed and reversible on its own (the
// "Private" preset just sets all three). Plus archived/locked notes, restricted categories, and the
// deletion-delay trash, alongside the publish actions that exclude release-blocked content.

import { useCallback, useEffect, useMemo, useState } from 'react';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import { PageHeader } from '../components/PageHeader';
import type { NoteRef, PolicyOverview } from '../types';
import './PrivacyView.css';

/** A removable capability chip: click to clear that restriction. */
function CapChip({
  label, busy, onRemove,
}: { label: string; busy: boolean; onRemove: () => void }) {
  return (
    <button className="pv-cap" disabled={busy} onClick={onRemove} title={`Remove "${label}" restriction`}>
      {label} <span aria-hidden>×</span>
    </button>
  );
}

/** Merge multiple per-capability NoteRef arrays into one ordered list with cap info per entry. */
function mergeRestricted(
  lists: Array<{ refs: NoteRef[]; cap: string }>,
): Array<{ id: string; title: string; caps: string[] }> {
  const map = new Map<string, { id: string; title: string; caps: string[] }>();
  for (const { refs, cap } of lists) {
    for (const ref of refs) {
      if (!map.has(ref.id)) map.set(ref.id, { id: ref.id, title: ref.title, caps: [] });
      map.get(ref.id)!.caps.push(cap);
    }
  }
  return [...map.values()].sort((a, b) => a.title.localeCompare(b.title));
}

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
  const [confirmDeleteAll, setConfirmDeleteAll] = useState(false);

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
      setNotice(`Published ${report.published_notes} note${report.published_notes !== 1 ? 's' : ''} (${report.excluded_private} withheld) → ${report.index_path}`);
    });
  }, [act]);

  const refreshGitignore = useCallback(() => act(async () => {
    const report = await api.refreshPrivateGitignore();
    setNotice(`${report.excluded_paths.length} path${report.excluded_paths.length !== 1 ? 's' : ''} excluded from git publish.`);
  }), [act]);

  // Merge per-capability lists — must be before the early returns (rules of hooks).
  const restrictedNotes = useMemo(() => mergeRestricted([
    { refs: policy?.hidden_notes ?? [], cap: 'hidden' },
    { refs: policy?.search_excluded_notes ?? [], cap: 'no search' },
    { refs: policy?.publish_excluded_notes ?? [], cap: 'no publish' },
  ]), [policy?.hidden_notes, policy?.search_excluded_notes, policy?.publish_excluded_notes]);

  const restrictedCategories = useMemo(() => mergeRestricted([
    { refs: (policy?.hidden_categories ?? []).map((name) => ({ id: name, title: name })), cap: 'hidden' },
    { refs: (policy?.search_excluded_categories ?? []).map((name) => ({ id: name, title: name })), cap: 'no search' },
    { refs: (policy?.publish_excluded_categories ?? []).map((name) => ({ id: name, title: name })), cap: 'no publish' },
  ]), [policy?.hidden_categories, policy?.search_excluded_categories, policy?.publish_excluded_categories]);

  if (error && !policy) return <div className="pv-state pv-error">{error}</div>;
  if (!policy) return <div className="pv-state">Loading policy…</div>;

  const capAction = (id: string, cap: string, isCategory = false) => {
    if (cap === 'hidden')     return isCategory ? () => act(() => api.setCategoryHidden(id, false), 'Shown.') : () => act(() => api.setNoteHidden(id, false), 'Shown.');
    if (cap === 'no search')  return isCategory ? () => act(() => api.setCategoryExcludeFromSearch(id, false), 'Searchable again.') : () => act(() => api.setNoteExcludeFromSearch(id, false), 'Searchable again.');
    /* no publish */          return isCategory ? () => act(() => api.setCategoryExcludeFromPublish(id, false), 'Publishable again.') : () => act(() => api.setNoteExcludeFromPublish(id, false), 'Publishable again.');
  };

  const nothing =
    restrictedNotes.length === 0 && policy.archived_notes.length === 0 &&
    policy.locked_notes.length === 0 && policy.pending_deletion.length === 0 &&
    restrictedCategories.length === 0;

  return (
    <div className="pv-root">
      <PageHeader title="Privacy & Policy">
        <button className="pv-btn" disabled={busy} onClick={publish}>Publish read-only site…</button>
        <button className="pv-btn" disabled={busy} onClick={refreshGitignore}>Refresh git exclusions</button>
      </PageHeader>

      {notice && <div className="pv-notice" onClick={() => setNotice(null)}>{notice}</div>}
      {error && <div className="pv-state pv-error">{error}</div>}

      {nothing && <div className="pv-state">Nothing is restricted. Notes are public, unlocked, and active.</div>}

      {policy.pending_deletion.length > 0 && (
        <section className="pv-section">
          <div className="pv-section-head">
            <h3 className="pv-section-title">Trash · pending deletion ({policy.pending_deletion.length})</h3>
            <div className="pv-section-actions">
              <button className="pv-link-btn" disabled={busy}
                onClick={() => act(async () => { const ids = await api.purgeExpired(); setNotice(`Swept ${ids.length} expired note${ids.length !== 1 ? 's' : ''}.`); })}>
                Sweep now
              </button>
              {confirmDeleteAll ? (
                <span className="pv-confirm-inline">
                  Delete all {policy.pending_deletion.length} now?{' '}
                  <button className="pv-link-btn pv-link-btn--danger" disabled={busy}
                    onClick={() => act(async () => { const ids = await api.purgeAllTrash(); setConfirmDeleteAll(false); setNotice(`Permanently deleted ${ids.length} note${ids.length !== 1 ? 's' : ''}.`); })}>
                    Confirm
                  </button>
                  {' '}
                  <button className="pv-link-btn" disabled={busy} onClick={() => setConfirmDeleteAll(false)}>Cancel</button>
                </span>
              ) : (
                <button className="pv-link-btn pv-link-btn--danger" disabled={busy} onClick={() => setConfirmDeleteAll(true)}>
                  Delete immediately
                </button>
              )}
            </div>
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

      {restrictedNotes.length > 0 && (
        <section className="pv-section">
          <h3 className="pv-section-title">Notes · privacy restrictions ({restrictedNotes.length})</h3>
          <p className="pv-hint">Click a tag to remove that restriction individually, or open the note to adjust all flags.</p>
          {restrictedNotes.map((n) => (
            <div key={n.id} className="pv-row">
              <button className="pv-name" onClick={() => openEditor(n.id)}>{n.title || '(untitled)'}</button>
              <span className="pv-caps">
                {n.caps.map((cap) => (
                  <CapChip key={cap} label={cap} busy={busy} onRemove={capAction(n.id, cap)} />
                ))}
              </span>
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

      {restrictedCategories.length > 0 && (
        <section className="pv-section">
          <h3 className="pv-section-title">Categories · privacy restrictions ({restrictedCategories.length})</h3>
          <p className="pv-hint">Applies to the category's notes. Click a tag to remove that restriction.</p>
          {restrictedCategories.map((c) => (
            <div key={c.id} className="pv-row">
              <span className="pv-name pv-name-static">#{c.title}</span>
              <span className="pv-caps">
                {c.caps.map((cap) => (
                  <CapChip key={cap} label={cap} busy={busy} onRemove={capAction(c.id, cap, true)} />
                ))}
              </span>
            </div>
          ))}
        </section>
      )}
    </div>
  );
}
