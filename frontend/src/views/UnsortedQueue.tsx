// Notebox — capture surface showing unsorted notes by default, with an "All notes" toggle.
// The sidebar badge always reflects the unsorted-only count regardless of the filter.

import { useCallback, useEffect, useMemo, useState } from 'react';
import { api } from '../lib/api';
import { displayTitle } from '../lib/utils';
import { useStore } from '../lib/store';
import { Icon } from '../components/Icon';
import { PageHeader } from '../components/PageHeader';
import type { NoteDto, ObjectType, TimelineDateField, NoteVisibility } from '../types';
import './UnsortedQueue.css';

const SORT_FIELDS: { id: TimelineDateField; label: string }[] = [
  { id: 'created', label: 'Created' },
  { id: 'updated', label: 'Updated' },
  { id: 'scheduled', label: 'Scheduled' },
  { id: 'completed', label: 'Completed' },
];

type FilterMode = 'unsorted' | 'all' | 'uncategorized';

const OBJECT_TYPE_LABELS: Record<ObjectType | 'all', string> = {
  all: 'All types', note: 'Note', quote: 'Quote', reference: 'Reference',
  todo: 'Todo', qa: 'Q&A', commentary: 'Commentary', table: 'Table',
  picture: 'Picture', drawing: 'Drawing', code: 'Code',
};

const ALL_OBJECT_TYPES: Array<ObjectType | 'all'> = [
  'all', 'note', 'quote', 'reference', 'todo', 'qa',
  'table', 'picture', 'drawing', 'code',
];

// Sort key (epoch ms) for a note on the chosen date field; null when the date is absent.
function noteSortKey(note: NoteDto, field: TimelineDateField): number | null {
  const dates = note.metadata.dates;
  const raw = field === 'created'
    ? dates.created
    : field === 'updated'
      ? dates.updated
      : field === 'scheduled'
        ? dates.scheduled?.date
        : dates.completed?.date;
  if (!raw) return null;
  const parsed = Date.parse(raw);
  return Number.isNaN(parsed) ? null : parsed;
}

interface NewNoteFormProps {
  onCreate: (note: NoteDto) => void;
}

function NewNoteForm({ onCreate }: NewNoteFormProps) {
  const [title, setTitle] = useState('');
  const [busy, setBusy] = useState(false);
  const [vanishing, setVanishing] = useState(false);
  const [vanishDays, setVanishDays] = useState(180);

  useEffect(() => {
    api.getBookConfig()
      .then((config) => setVanishDays(config.cleanup.default_vanish_days))
      .catch(() => {});
  }, []);

  const submit = async () => {
    const t = title.trim();
    if (!t) return;
    setBusy(true);
    try {
      const note = await api.createNote('note', t, undefined, {
        vanishing,
        vanish_days: vanishing ? vanishDays : undefined,
      });
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
      <label className="uq-sort">
        <input
          type="checkbox"
          checked={vanishing}
          onChange={(e) => setVanishing(e.target.checked)}
          disabled={busy}
        />
        Vanishing
      </label>
      {vanishing && (
        <label className="uq-sort">
          <span className="uq-sort-label">Days</span>
          <input
            className="uq-sort-select"
            type="number"
            min={1}
            value={vanishDays}
            onChange={(e) => setVanishDays(Math.max(1, Number(e.target.value) || 1))}
            disabled={busy}
          />
        </label>
      )}
    </div>
  );
}

export function UnsortedQueue() {
  const { openEditor, setUnsortedCount } = useStore();
  const [notes, setNotes] = useState<NoteDto[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showForm, setShowForm] = useState(false);
  const [filterMode, setFilterMode] = useState<FilterMode>('unsorted');
  const [visibility, setVisibility] = useState<NoteVisibility>('active');
  const [sortField, setSortField] = useState<TimelineDateField>('updated');
  const [sortDir, setSortDir] = useState<'desc' | 'asc'>('desc');
  const [typeFilter, setTypeFilter] = useState<ObjectType | 'all'>('all');

  const refresh = useCallback(() => {
    setLoading(true);
    const fetch = visibility === 'active'
      ? (filterMode === 'unsorted' ? api.unsortedNotes() : api.listNotes('active'))
      : api.listNotes(visibility);
    fetch
      .then((ns) => {
        setNotes(ns);
        if (filterMode !== 'unsorted' || visibility !== 'active') {
          api.unsortedNotes().then((us) => setUnsortedCount(us.length)).catch(console.error);
        } else {
          setUnsortedCount(ns.length);
        }
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [filterMode, visibility, setUnsortedCount]);

  useEffect(() => { refresh(); }, [refresh]);
  useEffect(() => {
    if (visibility !== 'active') setShowForm(false);
  }, [visibility]);

  const handleCreate = useCallback((note: NoteDto) => {
    setNotes((prev) => [note, ...prev]);
    setUnsortedCount(notes.length + 1);
    setShowForm(false);
    openEditor(note.id, 'edit');
  }, [openEditor, setUnsortedCount, notes.length]);

  const sortedNotes = useMemo(() => {
    const direction = sortDir === 'asc' ? 1 : -1;
    const modeFiltered = visibility !== 'active'
      ? notes
      : filterMode === 'uncategorized'
      ? notes.filter((n) => n.categories.length === 0)
      : notes;
    const sorted = [...modeFiltered].sort((a, b) => {
      const ka = noteSortKey(a, sortField);
      const kb = noteSortKey(b, sortField);
      if (ka === null && kb === null) return 0;
      if (ka === null) return 1;
      if (kb === null) return -1;
      return (ka - kb) * direction;
    });
    return typeFilter === 'all' ? sorted : sorted.filter((n) => n.type === typeFilter);
  }, [notes, filterMode, visibility, sortField, sortDir, typeFilter]);

  if (loading) return <div className="uq-state">Loading…</div>;
  if (error) return <div className="uq-state uq-error">{error}</div>;

  return (
    <div className="uq-root">
      <PageHeader title="Notebox">
          <div className="uq-filter-toggle">
            <button
              className={`uq-filter-btn ${filterMode === 'unsorted' ? 'active' : ''}`}
              onClick={() => setFilterMode('unsorted')}
            >
              Unsorted
            </button>
            <button
              className={`uq-filter-btn ${filterMode === 'all' ? 'active' : ''}`}
              onClick={() => setFilterMode('all')}
            >
              All notes
            </button>
            <button
              className={`uq-filter-btn ${filterMode === 'uncategorized' ? 'active' : ''}`}
              onClick={() => setFilterMode('uncategorized')}
              disabled={visibility !== 'active'}
            >
              Uncategorized
            </button>
          </div>
          <div className="uq-filter-toggle">
            <button
              className={`uq-filter-btn ${visibility === 'active' ? 'active' : ''}`}
              onClick={() => setVisibility('active')}
            >
              Active
            </button>
            <button
              className={`uq-filter-btn ${visibility === 'archived' ? 'active' : ''}`}
              onClick={() => setVisibility('archived')}
            >
              Archived
            </button>
            <button
              className={`uq-filter-btn ${visibility === 'trash' ? 'active' : ''}`}
              onClick={() => setVisibility('trash')}
            >
              Trash
            </button>
          </div>
          <label className="uq-sort">
            <span className="uq-sort-label">Sort</span>
            <select
              className="uq-sort-select"
              value={sortField}
              onChange={(e) => setSortField(e.target.value as TimelineDateField)}
            >
              {SORT_FIELDS.map((f) => <option key={f.id} value={f.id}>{f.label}</option>)}
            </select>
            <button
              className="uq-sort-dir"
              type="button"
              title={sortDir === 'desc' ? 'Newest first' : 'Oldest first'}
              aria-label={sortDir === 'desc' ? 'Newest first' : 'Oldest first'}
              onClick={() => setSortDir((d) => (d === 'desc' ? 'asc' : 'desc'))}
            >
              <Icon name={sortDir === 'desc' ? 'arrow_downward' : 'arrow_upward'} size={16} />
            </button>
          </label>
          <label className="uq-sort">
            <span className="uq-sort-label">Type</span>
            <select
              className="uq-sort-select"
              value={typeFilter}
              onChange={(e) => setTypeFilter(e.target.value as ObjectType | 'all')}
            >
              {ALL_OBJECT_TYPES.map((t) => (
                <option key={t} value={t}>{OBJECT_TYPE_LABELS[t]}</option>
              ))}
            </select>
          </label>
          {visibility === 'active' && (
            <button className="uq-add-btn" onClick={() => setShowForm((s) => !s)}>
              {showForm ? 'Cancel' : '+ New Note'}
            </button>
          )}
      </PageHeader>

      {showForm && visibility === 'active' && (
        <NewNoteForm onCreate={handleCreate} />
      )}

      {sortedNotes.length === 0 && !showForm ? (
        <div className="uq-empty">
          {visibility === 'archived' && <p>No archived notes.</p>}
          {visibility === 'trash' && <p>Trash is empty.</p>}
          {visibility === 'active' && filterMode === 'unsorted' && <p>All caught up! Every note has been organised.</p>}
          {visibility === 'active' && filterMode === 'all' && <p>No notes yet. Capture your first thought below.</p>}
          {visibility === 'active' && filterMode === 'uncategorized' && <p>No uncategorized notes — all notes have at least one category.</p>}
          {visibility === 'active' && (
            <button className="uq-add-btn" onClick={() => setShowForm(true)}>+ Capture a thought</button>
          )}
        </div>
      ) : (
        <div className="uq-list">
          {sortedNotes.map((note) => (
            <div
              key={note.id}
              className="uq-card selectable"
              onClick={() => openEditor(note.id)}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => e.key === 'Enter' && openEditor(note.id)}
            >
              <div className="uq-card-header">
                <span className="uq-card-title">{displayTitle(note.title, note.summary, note.body)}</span>
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
