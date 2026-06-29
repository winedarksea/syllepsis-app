// Collapsible metadata editor for a note: accordion sections with icon headers,
// chip-based workflow controls, 2×2 dates grid, and privacy toggle chips.
// Edits are applied to a NoteDto copy via `onChange`; the parent editor persists them
// through the normal updateNote save path.

import { useMemo, useState } from 'react';
import { useStore } from '../lib/store';
import { api } from '../lib/api';
import { Icon } from '../components/Icon';
import { WorldLocationHelper } from '../components/WorldLocationHelper';
import type {
  NoteDto, Category, LockMode, PriorKind, PriorRef, ClassificationKind, Priority, FlexDate, NoteStatus,
  NoteEmbeddingDetails,
} from '../types';

const CLASSIFICATION_KINDS: ClassificationKind[] = [
  'note', 'qa', 'reference', 'quote', 'code', 'todo', 'idea',
  'hypothesis', 'factual_claim', 'rule_or_requirement', 'principle', 'preference',
  'procedure', 'context', 'analysis_or_interpretation', 'narrative',
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

function formatDateShort(iso?: string): string {
  if (!iso) return '';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
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

function AdvancedContent({ embeddingDetails }: { embeddingDetails: NoteEmbeddingDetails | null }) {
  if (!embeddingDetails) return <div className="editor-advanced-empty">Embedding details unavailable.</div>;
  return (
    <div className="editor-embedding-details">
      <div>Status: {embeddingDetails.status}</div>
      <div>Model: {embeddingDetails.model_id ?? 'unknown'}</div>
      <div>Dimensions: {embeddingDetails.dimensions ?? 'unknown'}</div>
      <VectorPreview label="Summary vector" vector={embeddingDetails.summary_vector ?? null} />
      <VectorPreview label="Body vector" vector={embeddingDetails.full_note_vector ?? null} />
    </div>
  );
}

interface SectionHeadProps {
  icon: string;
  label: string;
  isOpen: boolean;
  summary?: string;
  onToggle: () => void;
}

function DateField({
  label,
  value,
  onChange,
}: {
  label: string;
  value?: FlexDate;
  onChange: (d: FlexDate | undefined) => void;
}) {
  const [picking, setPicking] = useState(false);
  const hasValue = Boolean(value?.date);

  if (!hasValue && !picking) {
    return (
      <div className="meta-date">
        <span className="dp-date-label">{label}</span>
        <button className="dp-date-empty" onClick={() => setPicking(true)}>—</button>
      </div>
    );
  }

  return (
    <label className="meta-date">
      <span className="dp-date-label">{label}</span>
      <div className="dp-date-row">
        <input
          autoFocus={picking}
          type="date"
          value={flexDateValue(value)}
          onChange={(e) => onChange(makeFlexDate(e.target.value))}
          onBlur={() => { if (!value?.date) setPicking(false); }}
        />
        {hasValue && (
          <button
            className="dp-date-clear"
            onClick={() => onChange(undefined)}
            aria-label={`Clear ${label}`}
          >
            ×
          </button>
        )}
      </div>
    </label>
  );
}

function SectionHead({ icon, label, isOpen, summary, onToggle }: SectionHeadProps) {
  return (
    <button className="dp-section-head" onClick={onToggle} aria-expanded={isOpen}>
      <Icon name={icon} size={14} className="dp-section-icon" />
      <span className="meta-label">{label}</span>
      {!isOpen && summary && <span className="dp-summary-pill">{summary}</span>}
      <Icon
        name="chevron_right"
        size={14}
        className={`dp-section-chevron${isOpen ? ' dp-section-chevron--open' : ''}`}
      />
    </button>
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
  const [openSections, setOpenSections] = useState<Set<string>>(() => new Set());
  const [newCategory, setNewCategory] = useState('');

  const otherNotes = useMemo(
    () => allNotes.filter((n) => n.id !== note.id),
    [allNotes, note.id],
  );

  const patch = (partial: Partial<NoteDto>) => onChange({ ...note, ...partial });
  const patchMeta = (partial: Partial<NoteDto['metadata']>) =>
    onChange({ ...note, metadata: { ...note.metadata, ...partial } });

  const toggleSection = (id: string) => {
    setOpenSections((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  };

  const { hidden: lcHidden, exclude_from_search: lcNoSearch, exclude_from_publish: lcNoPublish } =
    note.metadata.lifecycle ?? {};
  const privacyCount = [lcHidden, lcNoSearch, lcNoPublish].filter(Boolean).length;
  const allPrivate = privacyCount === 3;
  const somePrivate = privacyCount > 0 && !allPrivate;

  const addCategory = () => {
    const name = newCategory.trim().replace(/^#/, '');
    if (name && !note.categories.includes(name)) {
      patch({ categories: [...note.categories, name] });
    }
    setNewCategory('');
  };
  const removeCategory = (name: string) =>
    patch({ categories: note.categories.filter((c) => c !== name) });

  // Sort position
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

  // Summary pills
  const catSummary = note.categories.length
    ? `${note.categories.length} ${note.categories.length === 1 ? 'category' : 'categories'}`
    : undefined;
  const scheduledDate = note.metadata.dates.scheduled;
  const isTodo = note.metadata.classification.kind === 'todo';
  const dateSummary = scheduledDate
    ? `${isTodo ? 'Due' : 'Scheduled'} ${formatDateShort(scheduledDate.date)}`
    : undefined;
  const statusSummary = note.metadata.status ? humanize(note.metadata.status) : undefined;

  return (
    <div className="meta-panel">
      <button className="meta-panel-toggle" onClick={() => setOpen((v) => !v)}>
        <Icon name={open ? 'expand_more' : 'chevron_right'} size={14} />
        {' '}Details &amp; metadata
      </button>

      {open && (
        <div className="meta-panel-body">

          {/* Workflow: status, priority, modifiers */}
          <section className="meta-section">
            <SectionHead
              icon="tune"
              label="Workflow"
              isOpen={openSections.has('workflow')}
              summary={statusSummary}
              onToggle={() => toggleSection('workflow')}
            />
            {openSections.has('workflow') && (
              <div className="dp-section-body">
                <div className="meta-row">
                  <button
                    className="dp-status-chip"
                    aria-pressed={!note.metadata.status}
                    onClick={() => patchMeta({ status: undefined })}
                  >
                    No status
                  </button>
                  {NOTE_STATUSES.map((s) => (
                    <button
                      key={s}
                      className="dp-status-chip"
                      aria-pressed={note.metadata.status === s}
                      onClick={() => patchMeta({
                        status: note.metadata.status === s ? undefined : s,
                      })}
                    >
                      {humanize(s)}
                    </button>
                  ))}
                </div>

                <div className="dp-priority-group">
                  {PRIORITIES.map((p) => (
                    <button
                      key={p}
                      className="dp-priority-btn"
                      aria-pressed={note.metadata.classification.priority === p}
                      onClick={() => patchMeta({
                        classification: { ...note.metadata.classification, priority: p },
                      })}
                    >
                      {humanize(p)}
                    </button>
                  ))}
                </div>

                <div className="meta-row">
                  <button
                    className={`dp-modifier-chip${note.metadata.classification.starred ? ' dp-modifier-chip--active' : ''}`}
                    onClick={() => patchMeta({
                      classification: { ...note.metadata.classification, starred: !note.metadata.classification.starred },
                    })}
                    title="Starred"
                  >
                    <Icon name="star" size={14} fill={note.metadata.classification.starred} />
                    Starred
                  </button>
                  {!['picture', 'drawing'].includes(note.type) && (
                    <button
                      className={`dp-modifier-chip${note.metadata.lifecycle?.archived ? ' dp-modifier-chip--active' : ''}`}
                      onClick={() => patchMeta({
                        lifecycle: { ...note.metadata.lifecycle, archived: !note.metadata.lifecycle?.archived },
                      })}
                    >
                      Archived
                    </button>
                  )}
                  <select
                    className="dp-lock-select"
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
                </div>

                {isTodo && (
                  <p className="meta-hint">
                    Use <code>waiting:note-id</code> or <code>blocked-by:note-id</code> in the body to link tasks.
                    The note id is the <code>id:</code> field from the note's frontmatter.
                  </p>
                )}
              </div>
            )}
          </section>

          {/* Categories */}
          <section className="meta-section">
            <SectionHead
              icon="tag"
              label="Categories"
              isOpen={openSections.has('categories')}
              summary={catSummary}
              onToggle={() => toggleSection('categories')}
            />
            {openSections.has('categories') && (
              <div className="dp-section-body">
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
              </div>
            )}
          </section>

          {/* Dates: 2×2 grid */}
          <section className="meta-section">
            <SectionHead
              icon="calendar_today"
              label={isTodo ? 'Task dates' : 'Dates'}
              isOpen={openSections.has('dates')}
              summary={dateSummary}
              onToggle={() => toggleSection('dates')}
            />
            {openSections.has('dates') && (
              <div className="dp-section-body">
                <div className="dp-dates-grid">
                  <DateField
                    label={isTodo ? 'Due' : 'Scheduled'}
                    value={note.metadata.dates.scheduled}
                    onChange={(d) => patchMeta({ dates: { ...note.metadata.dates, scheduled: d } })}
                  />
                  <DateField
                    label={isTodo ? 'Done' : 'Completed'}
                    value={note.metadata.dates.completed}
                    onChange={(d) => patchMeta({ dates: { ...note.metadata.dates, completed: d } })}
                  />
                  <div className="meta-date-readonly">
                    Created
                    <span>{formatTimestamp(note.metadata.dates.created)}</span>
                  </div>
                  <div className="meta-date-readonly">
                    Updated
                    <span>{formatTimestamp(note.metadata.dates.updated)}</span>
                  </div>
                </div>
              </div>
            )}
          </section>

          {/* Location */}
          <section className="meta-section">
            <SectionHead
              icon="place"
              label="Location"
              isOpen={openSections.has('location')}
              onToggle={() => toggleSection('location')}
            />
            {openSections.has('location') && (
              <div className="dp-section-body">
                <input
                  className="meta-input"
                  value={note.location ?? ''}
                  onChange={(e) => patch({ location: e.target.value || undefined })}
                  placeholder='e.g. "Tokyo", "48.85,2.35", or "earth/48.85,2.35"'
                />
                <WorldLocationHelper onApply={(token) => patch({ location: token })} />
              </div>
            )}
          </section>

          {/* Position */}
          <section className="meta-section">
            <SectionHead
              icon="sort"
              label="Position"
              isOpen={openSections.has('position')}
              onToggle={() => toggleSection('position')}
            />
            {openSections.has('position') && (
              <div className="dp-section-body">
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
              </div>
            )}
          </section>

          {/* Classification */}
          <section className="meta-section">
            <SectionHead
              icon="label"
              label="Classification"
              isOpen={openSections.has('classification')}
              summary={humanize(note.metadata.classification.kind)}
              onToggle={() => toggleSection('classification')}
            />
            {openSections.has('classification') && (
              <div className="dp-section-body">
                <div className="meta-row">
                  <select
                    value={note.metadata.classification.kind}
                    onChange={(e) => patchMeta({
                      classification: { ...note.metadata.classification, kind: e.target.value as ClassificationKind },
                    })}
                  >
                    {CLASSIFICATION_KINDS.map((s) => <option key={s} value={s}>{humanize(s)}</option>)}
                  </select>
                </div>
              </div>
            )}
          </section>

          {/* Privacy: toggle chips replacing checkbox/preset pattern */}
          <section className="meta-section">
            <SectionHead
              icon="lock"
              label="Privacy"
              isOpen={openSections.has('privacy')}
              summary={privacyCount > 0 ? `${privacyCount} cap${privacyCount !== 1 ? 's' : ''} set` : undefined}
              onToggle={() => toggleSection('privacy')}
            />
            {openSections.has('privacy') && (
              <div className="dp-section-body">
                <div className="meta-row">
                  <button
                    className={`dp-privacy-chip dp-privacy-preset${allPrivate ? ' dp-privacy-chip--on' : somePrivate ? ' dp-privacy-chip--partial' : ''}`}
                    onClick={() => patchMeta({
                      lifecycle: {
                        ...note.metadata.lifecycle,
                        hidden: !allPrivate,
                        exclude_from_search: !allPrivate,
                        exclude_from_publish: !allPrivate,
                      },
                    })}
                    title="Set all three — hidden, no search, no publish"
                  >
                    Private
                  </button>
                  <button
                    className={`dp-privacy-chip${lcHidden ? ' dp-privacy-chip--on' : ''}`}
                    onClick={() => patchMeta({
                      lifecycle: { ...note.metadata.lifecycle, hidden: !lcHidden },
                    })}
                    title="Not shown in default views or exports"
                  >
                    Hidden
                  </button>
                  <button
                    className={`dp-privacy-chip${lcNoSearch ? ' dp-privacy-chip--on' : ''}`}
                    onClick={() => patchMeta({
                      lifecycle: { ...note.metadata.lifecycle, exclude_from_search: !lcNoSearch },
                    })}
                    title="Excluded from search and AI retrieval"
                  >
                    No search
                  </button>
                  <button
                    className={`dp-privacy-chip${lcNoPublish ? ' dp-privacy-chip--on' : ''}`}
                    onClick={() => patchMeta({
                      lifecycle: { ...note.metadata.lifecycle, exclude_from_publish: !lcNoPublish },
                    })}
                    title="Withheld from the published site (gitignored)"
                  >
                    No publish
                  </button>
                </div>
              </div>
            )}
          </section>

          {/* Advanced: embedding details */}
          <section className="meta-section">
            <SectionHead
              icon="memory"
              label="Advanced"
              isOpen={openSections.has('advanced')}
              onToggle={() => toggleSection('advanced')}
            />
            {openSections.has('advanced') && (
              <div className="dp-section-body editor-advanced-meta">
                <AdvancedContent embeddingDetails={embeddingDetails} />
              </div>
            )}
          </section>

        </div>
      )}
    </div>
  );
}
