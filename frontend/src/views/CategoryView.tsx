// Shows all notes assigned to a single category. Reads unsorted notes filtered client-side
// so the category header details (long_name, icon) are immediately available.
// Includes an inline category editor for icon, long name, and heading level.

import { useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api';
import { displayTitle } from '../lib/utils';
import { useStore } from '../lib/store';
import { Icon } from '../components/Icon';
import type { NoteDto, Category } from '../types';
import './CategoryView.css';

const HEADING_LEVELS = [1, 2, 3, 4, 5, 6];

function CategoryEditor({ cat, onSave, onCancel }: { cat: Category; onSave: (updated: Category) => void; onCancel: () => void }) {
  const [longName, setLongName] = useState(cat.long_name || '');
  const [icon, setIcon] = useState(cat.icon || '');
  const [headingLevel, setHeadingLevel] = useState(cat.heading_level || 2);
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    setBusy(true);
    try {
      const updated: Category = { ...cat, long_name: longName.trim() || cat.name, icon: icon.trim() || undefined, heading_level: headingLevel };
      await api.createCategory(updated);
      onSave(updated);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="cv-edit-panel">
      <h3 className="cv-edit-title">Edit category</h3>
      <label className="cv-edit-field">
        <span>Display name</span>
        <input value={longName} onChange={(e) => setLongName(e.target.value)} placeholder={cat.name} />
      </label>
      <label className="cv-edit-field">
        <span>Icon (emoji or text)</span>
        <input value={icon} onChange={(e) => setIcon(e.target.value)} placeholder="📚" maxLength={4} />
        {icon && <span className="cv-edit-icon-preview">{icon}</span>}
      </label>
      <label className="cv-edit-field">
        <span>Heading level in book view</span>
        <select value={headingLevel} onChange={(e) => setHeadingLevel(Number(e.target.value))}>
          {HEADING_LEVELS.map((l) => <option key={l} value={l}>H{l}</option>)}
        </select>
      </label>
      <div className="cv-edit-actions">
        <button className="cv-edit-btn" onClick={onCancel} disabled={busy}>Cancel</button>
        <button className="cv-edit-btn cv-edit-btn-primary" onClick={submit} disabled={busy}>
          {busy ? 'Saving…' : 'Save'}
        </button>
      </div>
    </div>
  );
}

export function CategoryView() {
  const { activeCategory, openEditor, categories, setCategories } = useStore();
  const [notes, setNotes] = useState<NoteDto[]>([]);
  const [loadedForCategory, setLoadedForCategory] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);

  const cat = categories.find((c) => c.name === activeCategory) ?? null;

  const refresh = useCallback(() => {
    if (!activeCategory) return;
    api.unsortedNotes()
      .then((unsorted) => {
        const filtered = unsorted.filter((n) => n.categories.includes(activeCategory));
        setNotes(filtered.sort((a, b) =>
          b.metadata.dates.updated.localeCompare(a.metadata.dates.updated)
        ));
        setError(null);
        setLoadedForCategory(activeCategory);
      })
      .catch((e) => {
        setError(String(e));
        setLoadedForCategory(activeCategory);
      });
  }, [activeCategory]);

  useEffect(() => { refresh(); }, [refresh]);
  useEffect(() => { setEditing(false); }, [activeCategory]);

  const loading = !!activeCategory && loadedForCategory !== activeCategory;

  const handleSave = useCallback(async (_updated: Category) => {
    const cats = await api.allCategories();
    setCategories(cats);
    setEditing(false);
  }, [setCategories]);

  if (!activeCategory) {
    return <div className="cv-state">Select a category from the sidebar.</div>;
  }

  if (loading) return <div className="cv-state">Loading…</div>;
  if (error) return <div className="cv-state cv-error">{error}</div>;

  return (
    <div className="cv-root">
      <div className="cv-header">
        <div className="cv-title-row">
          {cat?.icon && <span className="cv-icon">{cat.icon}</span>}
          <h2 className="cv-title">{cat?.long_name || activeCategory}</h2>
          <span className="cv-count">{notes.length} note{notes.length !== 1 ? 's' : ''}</span>
          <button className="cv-edit-toggle" onClick={() => setEditing((v) => !v)} title="Edit category">
            <Icon name={editing ? 'close' : 'edit'} size={15} />
          </button>
        </div>
        {cat?.heading_level && !editing && (
          <span className="cv-heading-level">H{cat.heading_level}</span>
        )}
      </div>

      {editing && cat && (
        <CategoryEditor cat={cat} onSave={handleSave} onCancel={() => setEditing(false)} />
      )}

      {!editing && notes.length === 0 ? (
        <div className="cv-empty">No notes in this category yet.</div>
      ) : !editing ? (
        <div className="cv-list selectable">
          {notes.map((note) => (
            <div
              key={note.id}
              className="cv-card"
              onClick={() => openEditor(note.id)}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => e.key === 'Enter' && openEditor(note.id)}
            >
              <div className="cv-card-header">
                <span className="cv-card-title">{displayTitle(note.title, note.summary, note.body)}</span>
                <span className="cv-card-type">{note.type}</span>
              </div>
              {note.summary && <p className="cv-card-summary">{note.summary}</p>}
              {note.body && (
                <p className="cv-card-body">{note.body.slice(0, 180)}{note.body.length > 180 ? '…' : ''}</p>
              )}
              <div className="cv-card-meta">
                <span className={note.sorted ? 'cv-status cv-status-done' : 'cv-status cv-status-pending'}>
                  <Icon name={note.sorted ? 'check_circle' : 'radio_button_unchecked'} size={13} />
                  {note.sorted ? 'Sorted' : 'Unsorted'}
                </span>
                <span className="cv-card-date">{new Date(note.metadata.dates.updated).toLocaleDateString()}</span>
              </div>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
