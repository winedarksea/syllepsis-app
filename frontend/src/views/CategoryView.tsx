// Shows all notes assigned to a single category. Reads unsorted notes filtered client-side
// so the category header details (long_name, icon) are immediately available.

import { useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import { Icon } from '../components/Icon';
import type { NoteDto } from '../types';
import './CategoryView.css';

export function CategoryView() {
  const { activeCategory, openEditor, categories } = useStore();
  const [notes, setNotes] = useState<NoteDto[]>([]);
  const [loadedForCategory, setLoadedForCategory] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

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

  const loading = !!activeCategory && loadedForCategory !== activeCategory;

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
        </div>
        {cat?.heading_level && (
          <span className="cv-heading-level">H{cat.heading_level}</span>
        )}
      </div>

      {notes.length === 0 ? (
        <div className="cv-empty">No notes in this category yet.</div>
      ) : (
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
                <span className="cv-card-title">{note.title || '(untitled)'}</span>
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
      )}
    </div>
  );
}
