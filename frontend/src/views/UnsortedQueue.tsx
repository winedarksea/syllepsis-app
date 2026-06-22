// The capture and categorisation surface. Shows all unsorted notes, newest first.
// Clicking a card opens the note in the editor; "New Note" creates a quick capture.

import { useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import { Icon } from '../components/Icon';
import type { NoteDto } from '../types';
import './UnsortedQueue.css';

interface NewNoteFormProps {
  onCreate: (note: NoteDto) => void;
}

function NewNoteForm({ onCreate }: NewNoteFormProps) {
  const [title, setTitle] = useState('');
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    const t = title.trim();
    if (!t) return;
    setBusy(true);
    try {
      const note = await api.createNote('note', t);
      setTitle('');
      onCreate(note);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="uq-new-form">
      <input
        className="uq-new-input"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={(e) => e.key === 'Enter' && submit()}
        placeholder="Quick capture — type and press Enter…"
        disabled={busy}
        autoFocus
      />
      <button className="uq-new-btn" onClick={submit} disabled={busy || !title.trim()}>
        Add
      </button>
    </div>
  );
}

export function UnsortedQueue() {
  const { openEditor, setUnsortedCount } = useStore();
  const [notes, setNotes] = useState<NoteDto[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showForm, setShowForm] = useState(false);

  const refresh = useCallback(() => {
    api.unsortedNotes()
      .then((ns) => {
        setNotes(ns);
        setUnsortedCount(ns.length);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [setUnsortedCount]);

  useEffect(() => { refresh(); }, [refresh]);

  const handleCreate = useCallback((note: NoteDto) => {
    setNotes((prev) => [note, ...prev]);
    setUnsortedCount(notes.length + 1);
    setShowForm(false);
    openEditor(note.id);
  }, [openEditor, setUnsortedCount, notes.length]);

  if (loading) return <div className="uq-state">Loading…</div>;
  if (error) return <div className="uq-state uq-error">{error}</div>;

  return (
    <div className="uq-root">
      <div className="uq-header">
        <h2 className="uq-title">Unsorted</h2>
        <button className="uq-add-btn" onClick={() => setShowForm((s) => !s)}>
          {showForm ? 'Cancel' : '+ New Note'}
        </button>
      </div>

      {showForm && (
        <NewNoteForm onCreate={handleCreate} />
      )}

      {notes.length === 0 && !showForm ? (
        <div className="uq-empty">
          <p>All caught up! Every note has been organised.</p>
          <button className="uq-add-btn" onClick={() => setShowForm(true)}>+ Capture a thought</button>
        </div>
      ) : (
        <div className="uq-list">
          {notes.map((note) => (
            <div
              key={note.id}
              className="uq-card selectable"
              onClick={() => openEditor(note.id)}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => e.key === 'Enter' && openEditor(note.id)}
            >
              <div className="uq-card-header">
                <span className="uq-card-title">{note.title || '(untitled)'}</span>
                {note.metadata.classification.starred && (
                  <Icon name="star" size={14} fill className="uq-card-star" title="Starred" />
                )}
                <span className="uq-card-type">{note.type}</span>
              </div>
              {note.summary && (
                <p className="uq-card-summary">{note.summary}</p>
              )}
              {note.body && (
                <p className="uq-card-body">{note.body.slice(0, 200)}{note.body.length > 200 ? '…' : ''}</p>
              )}
              {note.categories.length > 0 && (
                <div className="uq-card-tags">
                  {note.categories.map((c) => (
                    <span key={c} className="uq-tag">#{c}</span>
                  ))}
                </div>
              )}
              <div className="uq-card-date">
                {new Date(note.metadata.dates.created).toLocaleDateString()}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
