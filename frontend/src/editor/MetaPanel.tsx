// Collapsible metadata editor for a note: categories, location, sort position (prior),
// classification, scheduling dates, and type-specific metadata.
// Edits are applied to a NoteDto copy via `onChange`; the parent editor persists them
// through the normal updateNote save path.

import { useEffect, useMemo, useRef, useState } from 'react';
import { useStore } from '../lib/store';
import { api } from '../lib/api';
import { WorldLocationHelper } from '../components/WorldLocationHelper';
import type {
  NoteDto, Category, LockMode, PriorKind, PriorRef, StatementType, Priority, FlexDate, NoteStatus,
  NoteEmbeddingDetails,
} from '../types';

const STATEMENT_TYPES: StatementType[] = [
  'hypothesis', 'factual_claim', 'rule_or_requirement', 'principle', 'preference',
  'procedure', 'context', 'analysis_or_interpretation', 'narrative', 'idea',
];
const PRIORITIES: Priority[] = ['standard', 'important', 'core'];
const NOTE_STATUSES: NoteStatus[] = [
  'open', 'active', 'needs_clarification', 'deferred', 'cancelled', 'done',
];
const PRIOR_KINDS: PriorKind[] = [
  'new_paragraph', 'same_paragraph', 'indented_new_paragraph', 'bullet_point', 'numbered_list',
];

function humanize(value: string): string {
  return value.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

function flexDateValue(d?: FlexDate): string {
  return d?.date ? d.date.slice(0, 10) : '';
}

function makeFlexDate(value: string): FlexDate | undefined {
  return value ? { date: value } : undefined;
}

function formatTimestamp(iso: string): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
}

function VectorPreview({ label, vector }: { label: string; vector: number[] | null }) {
  if (!vector) return <div>{label}: none</div>;
  return (
    <details className="editor-vector-preview">
      <summary>{label}: {vector.length} values</summary>
      <code>{vector.map((value) => Number(value).toFixed(4)).join(', ')}</code>
    </details>
  );
}

function AdvancedMetadata({ embeddingDetails }: { embeddingDetails: NoteEmbeddingDetails | null }) {
  return (
    <details className="editor-advanced-meta">
      <summary>Advanced</summary>
      {!embeddingDetails ? (
        <div className="editor-advanced-empty">Embedding details unavailable.</div>
      ) : (
        <div className="editor-embedding-details">
          <div>Status: {embeddingDetails.status}</div>
          <div>Model: {embeddingDetails.model_id ?? 'unknown'}</div>
          <div>Dimensions: {embeddingDetails.dimensions ?? 'unknown'}</div>
          <VectorPreview label="Summary vector" vector={embeddingDetails.summary_vector ?? null} />
          <VectorPreview label="Body vector" vector={embeddingDetails.full_note_vector ?? null} />
        </div>
      )}
    </details>
  );
}

interface Props {
  note: NoteDto;
  categories: Category[];
  allNotes: NoteDto[];
  embeddingDetails: NoteEmbeddingDetails | null;
  onChange: (next: NoteDto) => void;
}

export function MetaPanel({ note, categories, allNotes, embeddingDetails, onChange }: Props) {
  const { setActiveCategory, setView } = useStore();
  const [open, setOpen] = useState(false);
  const [newCategory, setNewCategory] = useState('');

  const otherNotes = useMemo(
    () => allNotes.filter((n) => n.id !== note.id),
    [allNotes, note.id],
  );

  const patch = (partial: Partial<NoteDto>) => onChange({ ...note, ...partial });
  const patchMeta = (partial: Partial<NoteDto['metadata']>) =>
    onChange({ ...note, metadata: { ...note.metadata, ...partial } });

  // Privacy preset — show indeterminate (−) when some but not all caps are set.
  const privacyPresetRef = useRef<HTMLInputElement>(null);
  const { hidden: lcHidden, exclude_from_search: lcNoSearch, exclude_from_publish: lcNoPublish } =
    note.metadata.lifecycle ?? {};
  const privacyCount = [lcHidden, lcNoSearch, lcNoPublish].filter(Boolean).length;
  const allPrivate = privacyCount === 3;
  const somePrivate = privacyCount > 0 && !allPrivate;
  useEffect(() => {
    if (privacyPresetRef.current) privacyPresetRef.current.indeterminate = somePrivate;
  }, [somePrivate]);

  const addCategory = () => {
    const name = newCategory.trim().replace(/^#/, '');
    if (name && !note.categories.includes(name)) {
      patch({ categories: [...note.categories, name] });
    }
    setNewCategory('');
  };
  const removeCategory = (name: string) =>
    patch({ categories: note.categories.filter((c) => c !== name) });

  // ── Sort position (prior) ──
  const priorMode: 'none' | 'category' | 'note' =
    !note.prior ? 'none' : 'category' in note.prior.target ? 'category' : 'note';

  const setPriorMode = (mode: 'none' | 'category' | 'note') => {
    if (mode === 'none') return patch({ prior: undefined });
    const kind: PriorKind = note.prior?.kind ?? 'new_paragraph';
    const target: PriorRef =
      mode === 'category'
        ? { category: categories[0]?.name ?? '' }
        : { note: otherNotes[0]?.id ?? '' };
    patch({ prior: { target, kind } });
  };
  const setPriorTarget = (target: PriorRef) => {
    if (!note.prior) return;
    patch({ prior: { ...note.prior, target } });
  };
  const setPriorKind = (kind: PriorKind) => {
    if (!note.prior) return;
    patch({ prior: { ...note.prior, kind } });
  };

  return (
    <div className="meta-panel">
      <button className="meta-panel-toggle" onClick={() => setOpen((v) => !v)}>
        {open ? '▾' : '▸'} Details &amp; metadata
      </button>

      {open && (
        <div className="meta-panel-body">
          {/* Categories */}
          <section className="meta-section">
            <label className="meta-label">Categories</label>
            <div className="meta-chips">
              {note.categories.map((c) => (
                <span key={c} className="meta-chip">
                  <button
                    className="meta-chip-link"
                    onClick={() => { setActiveCategory(c); setView('category'); }}
                  >
                    #{c}
                  </button>
                  <button className="meta-chip-x" onClick={() => removeCategory(c)} aria-label={`Remove ${c}`}>×</button>
                </span>
              ))}
              <input
                className="meta-chip-input"
                value={newCategory}
                onChange={(e) => setNewCategory(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); addCategory(); } }}
                onBlur={addCategory}
                placeholder="add category…"
              />
            </div>
          </section>

          {/* Location */}
          <section className="meta-section">
            <label className="meta-label">Location</label>
            <input
              className="meta-input"
              value={note.location ?? ''}
              onChange={(e) => patch({ location: e.target.value || undefined })}
              placeholder='e.g. "Tokyo", "48.85,2.35", or "earth/48.85,2.35"'
            />
            <WorldLocationHelper onApply={(token) => patch({ location: token })} />
          </section>

          {/* Sort position */}
          <section className="meta-section">
            <label className="meta-label">Position</label>
            <div className="meta-row">
              <select value={priorMode} onChange={(e) => setPriorMode(e.target.value as typeof priorMode)}>
                <option value="none">Unsorted</option>
                <option value="category">Under category</option>
                <option value="note">After note</option>
              </select>
              {priorMode === 'category' && (
                <select
                  value={'category' in note.prior!.target ? note.prior!.target.category : ''}
                  onChange={(e) => setPriorTarget({ category: e.target.value })}
                >
                  {categories.length === 0 && <option value="">(no categories)</option>}
                  {categories.map((c) => <option key={c.name} value={c.name}>{c.long_name || c.name}</option>)}
                </select>
              )}
              {priorMode === 'note' && (
                <select
                  value={'note' in note.prior!.target ? note.prior!.target.note : ''}
                  onChange={(e) => setPriorTarget({ note: e.target.value })}
                >
                  {otherNotes.length === 0 && <option value="">(no other notes)</option>}
                  {otherNotes.map((n) => <option key={n.id} value={n.id}>{n.title || '(untitled)'}</option>)}
                </select>
              )}
              {priorMode !== 'none' && (
                <select value={note.prior!.kind} onChange={(e) => setPriorKind(e.target.value as PriorKind)}>
                  {PRIOR_KINDS.map((k) => <option key={k} value={k}>{humanize(k)}</option>)}
                </select>
              )}
            </div>
          </section>

          {/* Classification */}
          <section className="meta-section">
            <label className="meta-label">Classification</label>
            <div className="meta-row">
              <select
                value={note.metadata.classification.statement_type}
                onChange={(e) => patchMeta({
                  classification: { ...note.metadata.classification, statement_type: e.target.value as StatementType },
                })}
              >
                {STATEMENT_TYPES.map((s) => <option key={s} value={s}>{humanize(s)}</option>)}
              </select>
              <select
                value={note.metadata.classification.priority}
                onChange={(e) => patchMeta({
                  classification: { ...note.metadata.classification, priority: e.target.value as Priority },
                })}
              >
                {PRIORITIES.map((p) => <option key={p} value={p}>{humanize(p)}</option>)}
              </select>
              <label className="meta-checkbox">
                <input
                  type="checkbox"
                  checked={note.metadata.classification.starred}
                  onChange={(e) => patchMeta({
                    classification: { ...note.metadata.classification, starred: e.target.checked },
                  })}
                />
                Starred
              </label>
              {!['picture', 'drawing'].includes(note.type) && (
                <label className="meta-checkbox">
                  <input
                    type="checkbox"
                    checked={note.metadata.lifecycle?.archived ?? false}
                    onChange={(e) => patchMeta({
                      lifecycle: { ...note.metadata.lifecycle, archived: e.target.checked },
                    })}
                  />
                  Archived
                </label>
              )}
              <select
                value={note.metadata.lifecycle?.lock ?? 'none'}
                onChange={(e) => {
                  api.setNoteLock(note.id, e.target.value as LockMode)
                    .then((updated) => onChange(updated))
                    .catch(() => {});
                }}
              >
                <option value="none">No lock</option>
                <option value="unlock_delay">Unlock delay (24 h)</option>
                <option value="fact_check_gate">Fact-check gate</option>
              </select>
              <select
                value={note.metadata.status ?? ''}
                onChange={(e) => patchMeta({
                  status: e.target.value ? e.target.value as NoteStatus : undefined,
                })}
              >
                <option value="">No status</option>
                {NOTE_STATUSES.map((status) => (
                  <option key={status} value={status}>{humanize(status)}</option>
                ))}
              </select>
            </div>
          </section>

          {/* Privacy: "Private" is a preset (all three at once); each cap is also toggleable independently. */}
          <section className="meta-section">
            <label className="meta-label">Privacy</label>
            <div className="meta-row">
              <label className="meta-checkbox meta-privacy-preset" title="Set all three — hidden, no search, no publish">
                <input
                  ref={privacyPresetRef}
                  type="checkbox"
                  checked={allPrivate}
                  onChange={(e) => patchMeta({
                    lifecycle: {
                      ...note.metadata.lifecycle,
                      hidden: e.target.checked,
                      exclude_from_search: e.target.checked,
                      exclude_from_publish: e.target.checked,
                    },
                  })}
                />
                Private
              </label>
              <span className="meta-privacy-sep" aria-hidden>·</span>
              <label className="meta-checkbox" title="Not shown in default views or exports">
                <input
                  type="checkbox"
                  checked={lcHidden ?? false}
                  onChange={(e) => patchMeta({
                    lifecycle: { ...note.metadata.lifecycle, hidden: e.target.checked },
                  })}
                />
                Hidden
              </label>
              <label className="meta-checkbox" title="Excluded from search and AI retrieval">
                <input
                  type="checkbox"
                  checked={lcNoSearch ?? false}
                  onChange={(e) => patchMeta({
                    lifecycle: { ...note.metadata.lifecycle, exclude_from_search: e.target.checked },
                  })}
                />
                No search
              </label>
              <label className="meta-checkbox" title="Withheld from the published site (gitignored)">
                <input
                  type="checkbox"
                  checked={lcNoPublish ?? false}
                  onChange={(e) => patchMeta({
                    lifecycle: { ...note.metadata.lifecycle, exclude_from_publish: e.target.checked },
                  })}
                />
                No publish
              </label>
            </div>
          </section>

          {/* Dates */}
          <section className="meta-section">
            <label className="meta-label">{note.type === 'todo' ? 'Task dates' : 'Dates'}</label>
            <div className="meta-row">
              <label className="meta-date">
                {note.type === 'todo' ? 'Due' : 'Scheduled'}
                <input
                  type="date"
                  value={flexDateValue(note.metadata.dates.scheduled)}
                  onChange={(e) => patchMeta({
                    dates: { ...note.metadata.dates, scheduled: makeFlexDate(e.target.value) },
                  })}
                />
              </label>
              <label className="meta-date">
                {note.type === 'todo' ? 'Done' : 'Completed'}
                <input
                  type="date"
                  value={flexDateValue(note.metadata.dates.completed)}
                  onChange={(e) => patchMeta({
                    dates: { ...note.metadata.dates, completed: makeFlexDate(e.target.value) },
                  })}
                />
              </label>
              <div className="meta-date-readonly">
                Created
                <span>{formatTimestamp(note.metadata.dates.created)}</span>
              </div>
              <div className="meta-date-readonly">
                Updated
                <span>{formatTimestamp(note.metadata.dates.updated)}</span>
              </div>
            </div>
            {note.type === 'todo' && (
              <p className="meta-hint">
                Inline <code>due:</code>, <code>start:</code>, <code>done:</code> tokens in the body also set these via the syntax insert menu.
              </p>
            )}
          </section>

          {/* Task link syntax hints (for todo notes) */}
          {note.type === 'todo' && (
            <section className="meta-section">
              <label className="meta-label">Task links</label>
              <p className="meta-hint">
                Use <code>waiting:note-id</code> or <code>blocked-by:note-id</code> in the body to link tasks.
                The note id is the <code>id:</code> field from the note's frontmatter.
              </p>
            </section>
          )}
          <AdvancedMetadata embeddingDetails={embeddingDetails} />
        </div>
      )}
    </div>
  );
}
