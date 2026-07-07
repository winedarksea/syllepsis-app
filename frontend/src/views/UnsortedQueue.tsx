// Notebox — review surface showing unsorted notes by default, with an "All notes" toggle.
// The sidebar badge always reflects the unsorted-only count regardless of the filter.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api } from '../lib/api';
import { displayTitle } from '../lib/utils';
import { useStore } from '../lib/store';
import { Icon } from '../components/Icon';
import { PageHeader } from '../components/PageHeader';
import type { ClassificationKind, NoteDto, TimelineDateField } from '../types';
import './UnsortedQueue.css';

const SORT_FIELDS: { id: TimelineDateField; label: string }[] = [
  { id: 'created', label: 'Created' },
  { id: 'updated', label: 'Updated' },
  { id: 'scheduled', label: 'Scheduled' },
  { id: 'started', label: 'Started' },
  { id: 'due', label: 'Due' },
  { id: 'completed', label: 'Completed' },
];

type FilterMode = 'unsorted' | 'all' | 'uncategorized';

const WINDOW_SIZE = 30;

const CLASSIFICATION_LABELS: Record<ClassificationKind | 'all', string> = {
  all: 'All classifications',
  note: 'Note',
  qa: 'Q&A',
  reference: 'Reference',
  quote: 'Quote',
  code: 'Code',
  todo: 'Todo',
  idea: 'Idea',
  hypothesis: 'Hypothesis',
  factual_claim: 'Factual Claim',
  rule_or_requirement: 'Rule Or Requirement',
  principle: 'Principle',
  preference: 'Preference',
  procedure: 'Procedure',
  context: 'Context',
  analysis_or_interpretation: 'Analysis Or Interpretation',
  narrative: 'Narrative',
};

const ALL_CLASSIFICATIONS: Array<ClassificationKind | 'all'> = [
  'all', 'note', 'qa', 'reference', 'quote', 'code', 'todo', 'idea',
  'hypothesis', 'factual_claim', 'rule_or_requirement', 'principle', 'preference',
  'procedure', 'context', 'analysis_or_interpretation', 'narrative',
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
        : field === 'started'
          ? dates.started?.date
          : field === 'due'
            ? dates.due?.date
            : dates.completed?.date;
  if (!raw) return null;
  const parsed = Date.parse(raw);
  return Number.isNaN(parsed) ? null : parsed;
}

function mergeNotesById(primaryNotes: NoteDto[], secondaryNotes: NoteDto[]): NoteDto[] {
  const notesById = new Map<string, NoteDto>();
  [...primaryNotes, ...secondaryNotes].forEach((note) => notesById.set(note.id, note));
  return [...notesById.values()];
}

export function UnsortedQueue() {
  const { openEditor, setUnsortedCount } = useStore();
  const [notes, setNotes] = useState<NoteDto[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filterMode, setFilterMode] = useState<FilterMode>('unsorted');
  const [includeArchived, setIncludeArchived] = useState(false);
  const [includePrivate, setIncludePrivate] = useState(false);
  const [sortField, setSortField] = useState<TimelineDateField>('updated');
  const [sortDir, setSortDir] = useState<'desc' | 'asc'>('desc');
  const [classificationFilter, setClassificationFilter] = useState<ClassificationKind | 'all'>('all');
  const [filterOpen, setFilterOpen] = useState(false);
  const filterRef = useRef<HTMLDivElement | null>(null);

  // Windowing: the full note list can be large, so only the first `visibleCount` cards render,
  // growing by WINDOW_SIZE as the sentinel scrolls into view (same pattern as SearchView).
  const [visibleCount, setVisibleCount] = useState(WINDOW_SIZE);
  const sentinelRef = useRef<HTMLDivElement | null>(null);
  const observerRef = useRef<IntersectionObserver | null>(null);

  const refresh = useCallback(() => {
    setLoading(true);
    setError(null);
    const archivedNotesPromise = includeArchived ? api.listNotes('archived') : Promise.resolve([]);
    const privateNotesPromise = includePrivate ? api.listNotes('hidden') : Promise.resolve([]);
    Promise.all([api.listNotes('active'), archivedNotesPromise, privateNotesPromise, api.unsortedNotes()])
      .then(([activeNotes, archivedNotes, privateNotes, activeUnsortedNotes]) => {
        setNotes(mergeNotesById(mergeNotesById(activeNotes, archivedNotes), privateNotes));
        setUnsortedCount(activeUnsortedNotes.length);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [includeArchived, includePrivate, setUnsortedCount]);

  useEffect(() => { refresh(); }, [refresh]);

  // Close the filter popover on click-outside or Escape (pattern like the Sidebar "New" menu).
  useEffect(() => {
    if (!filterOpen) return;
    const onPointerDown = (e: MouseEvent) => {
      if (filterRef.current && !filterRef.current.contains(e.target as Node)) setFilterOpen(false);
    };
    const onKeyDown = (e: KeyboardEvent) => { if (e.key === 'Escape') setFilterOpen(false); };
    document.addEventListener('mousedown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('mousedown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [filterOpen]);

  const activeFilterCount =
    (classificationFilter !== 'all' ? 1 : 0) + (includeArchived ? 1 : 0) + (includePrivate ? 1 : 0);

  const sortedNotes = useMemo(() => {
    const direction = sortDir === 'asc' ? 1 : -1;
    const modeFiltered = filterMode === 'unsorted'
      ? notes.filter((n) => !n.sorted)
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
    return classificationFilter === 'all'
      ? sorted
      : sorted.filter((n) => n.metadata.classification.kind === classificationFilter);
  }, [notes, filterMode, sortField, sortDir, classificationFilter]);

  // Reset the window whenever the filtered/sorted set changes underneath it.
  useEffect(() => { setVisibleCount(WINDOW_SIZE); }, [sortedNotes]);

  // IntersectionObserver for windowed reveal (same pattern as SearchView).
  useEffect(() => {
    if (observerRef.current) observerRef.current.disconnect();
    if (visibleCount >= sortedNotes.length) return;
    observerRef.current = new IntersectionObserver(
      (entries) => {
        if (entries[0].isIntersecting) {
          setVisibleCount((c) => Math.min(c + WINDOW_SIZE, sortedNotes.length));
        }
      },
      { threshold: 0.1 },
    );
    if (sentinelRef.current) observerRef.current.observe(sentinelRef.current);
    return () => observerRef.current?.disconnect();
  }, [sortedNotes, visibleCount]);

  const shownNotes = sortedNotes.slice(0, visibleCount);

  if (loading) return <div className="uq-state">Loading…</div>;
  if (error) return <div className="uq-state uq-error">{error}</div>;

  return (
    <div className="uq-root">
      <PageHeader title="Notebox">
        <div className="uq-toolbar">
          <div className="uq-filter-toggle" aria-label="Note mode">
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
            >
              Uncategorized
            </button>
          </div>
          <div className="uq-toolbar-utility">
            <div className="uq-filter" ref={filterRef}>
              <button
                type="button"
                className={`uq-filter-toggle-btn ${activeFilterCount > 0 ? 'active' : ''}`}
                aria-expanded={filterOpen}
                aria-haspopup="true"
                onClick={() => setFilterOpen((v) => !v)}
              >
                <Icon name="filter_list" size={16} />
                <span>Filter{activeFilterCount > 0 ? ` · ${activeFilterCount}` : ''}</span>
              </button>
              {filterOpen && (
                <div className="uq-filter-popover" role="menu">
                  <label className="uq-filter-field">
                    <span className="uq-filter-label">Classification</span>
                    <select
                      className="uq-sort-select uq-classification-select"
                      value={classificationFilter}
                      aria-label="Classification"
                      onChange={(e) => setClassificationFilter(e.target.value as ClassificationKind | 'all')}
                    >
                      {ALL_CLASSIFICATIONS.map((t) => (
                        <option key={t} value={t}>{CLASSIFICATION_LABELS[t]}</option>
                      ))}
                    </select>
                  </label>
                  <label className="uq-filter-check">
                    <input
                      type="checkbox"
                      checked={includeArchived}
                      onChange={(e) => setIncludeArchived(e.target.checked)}
                    />
                    Include archived
                  </label>
                  <label className="uq-filter-check">
                    <input
                      type="checkbox"
                      checked={includePrivate}
                      onChange={(e) => setIncludePrivate(e.target.checked)}
                    />
                    Include private
                  </label>
                </div>
              )}
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
          </div>
        </div>
      </PageHeader>

      {sortedNotes.length === 0 ? (
        <div className="uq-empty">
          {filterMode === 'unsorted' && <p>All caught up! Every note has been organised.</p>}
          {filterMode === 'all' && <p>No notes to show.</p>}
          {filterMode === 'uncategorized' && <p>No uncategorized notes. All notes have at least one category.</p>}
        </div>
      ) : (
        <div className="uq-list">
          {shownNotes.map((note) => (
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
                {includeArchived && note.metadata.lifecycle?.archived && (
                  <span className="uq-card-type uq-card-type--archived">Archived</span>
                )}
                {includePrivate && note.metadata.lifecycle?.hidden && (
                  <span className="uq-card-type uq-card-type--private">Private</span>
                )}
                {note.pin_locked && (
                  <span className="uq-card-type uq-card-type--private" title="PIN-locked">
                    <Icon name="lock" size={11} /> Locked
                  </span>
                )}
                <span className="uq-card-type">{CLASSIFICATION_LABELS[note.metadata.classification.kind]}</span>
                {note.type !== 'note' && <span className="uq-card-type">{note.type}</span>}
              </div>
              {note.pin_locked ? (
                <p className="uq-card-summary uq-card-summary--locked">Locked — enter PIN to view</p>
              ) : (
                <>
                  {note.summary && (
                    <p className="uq-card-summary">{note.summary}</p>
                  )}
                  {note.body && (
                    <p className="uq-card-body">{note.body.slice(0, 200)}{note.body.length > 200 ? '…' : ''}</p>
                  )}
                </>
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
          {/* Intersection sentinel for windowed reveal */}
          {visibleCount < sortedNotes.length && (
            <div ref={sentinelRef} className="uq-sentinel" aria-hidden="true" />
          )}
        </div>
      )}
    </div>
  );
}
