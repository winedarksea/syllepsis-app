// Centralized privacy & lifecycle dashboard (Phase 6, privacy-security.md). One place to see and
// reverse every restriction in the book. Privacy is three independent capabilities — hidden,
// excluded-from-search, and excluded-from-publish — each listed and reversible on its own (the
// "Private" preset just sets all three). Plus archived and locked notes, and restricted categories.
// Publish configuration lives in Settings → Publishing; the publish action and Trash have their own
// homes (Book View header and the Trash view).

import { useCallback, useEffect, useMemo, useState } from 'react';
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
    policy.locked_notes.length === 0 && policy.pin_locked_notes.length === 0 &&
    restrictedCategories.length === 0;

  return (
    <div className="pv-root">
      <PageHeader title="Privacy & Policy" />

      <p className="pv-intro">
        Each note's three privacy capabilities — hidden, excluded from search, and excluded from
        publish — are set in the editor. Unlock and confirmation delays are configured in
        Settings → Privacy &amp; Security.
      </p>

      {notice && <div className="pv-notice" onClick={() => setNotice(null)}>{notice}</div>}
      {error && <div className="pv-state pv-error">{error}</div>}

      {nothing && <div className="pv-state">Nothing is restricted. Notes are public, unlocked, and active.</div>}

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

      {policy.pin_locked_notes.length > 0 && (
        <section className="pv-section">
          <h3 className="pv-section-title">PIN-locked notes ({policy.pin_locked_notes.length})</h3>
          <p className="pv-hint">
            Summary and body are encrypted. Open a note and enter the book's PIN in the Privacy section to view or unlock it.
          </p>
          {policy.pin_locked_notes.map((n) => (
            <div key={n.id} className="pv-row">
              <button className="pv-name" onClick={() => openEditor(n.id)}>{n.title || '(untitled)'}</button>
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
