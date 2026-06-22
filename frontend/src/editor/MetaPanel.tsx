// Collapsible metadata editor for a note: categories, location, sort position (prior),
// classification, and scheduling dates. Edits are applied to a NoteDto copy via `onChange`;
// the parent editor persists them through the normal updateNote save path.

import { useMemo, useState } from 'react';
import type {
  NoteDto, Category, PriorKind, PriorRef, StatementType, Priority, FlexDate,
} from '../types';

const STATEMENT_TYPES: StatementType[] = [
  'hypothesis', 'factual_claim', 'rule_or_requirement', 'principle', 'preference',
  'procedure', 'context', 'analysis_or_interpretation', 'narrative', 'idea',
];
const PRIORITIES: Priority[] = ['standard', 'important', 'core'];
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

interface Props {
  note: NoteDto;
  categories: Category[];
  allNotes: NoteDto[];
  onChange: (next: NoteDto) => void;
}

export function MetaPanel({ note, categories, allNotes, onChange }: Props) {
  const [open, setOpen] = useState(false);
  const [newCategory, setNewCategory] = useState('');

  const otherNotes = useMemo(
    () => allNotes.filter((n) => n.id !== note.id),
    [allNotes, note.id],
  );

  const patch = (partial: Partial<NoteDto>) => onChange({ ...note, ...partial });
  const patchMeta = (partial: Partial<NoteDto['metadata']>) =>
    onChange({ ...note, metadata: { ...note.metadata, ...partial } });

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
                  #{c}
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
              placeholder='Place name or "lat,lon" to pin on a map'
            />
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
            </div>
          </section>

          {/* Dates */}
          <section className="meta-section">
            <label className="meta-label">Dates</label>
            <div className="meta-row">
              <label className="meta-date">
                Scheduled
                <input
                  type="date"
                  value={flexDateValue(note.metadata.dates.scheduled)}
                  onChange={(e) => patchMeta({
                    dates: { ...note.metadata.dates, scheduled: makeFlexDate(e.target.value) },
                  })}
                />
              </label>
              <label className="meta-date">
                Completed
                <input
                  type="date"
                  value={flexDateValue(note.metadata.dates.completed)}
                  onChange={(e) => patchMeta({
                    dates: { ...note.metadata.dates, completed: makeFlexDate(e.target.value) },
                  })}
                />
              </label>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}
