// Collapsible metadata editor for a note: categories, location, sort position (prior),
// classification, scheduling dates, and type-specific metadata.
// Edits are applied to a NoteDto copy via `onChange`; the parent editor persists them
// through the normal updateNote save path.

import { useEffect, useMemo, useState } from 'react';
import { api } from '../lib/api';
import type {
  NoteDto, Category, PriorKind, PriorRef, StatementType, Priority, FlexDate, World,
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
  const [worlds, setWorlds] = useState<World[]>([]);
  const [showWorldHelper, setShowWorldHelper] = useState(false);
  const [worldId, setWorldId] = useState('');
  const [coordX, setCoordX] = useState('');
  const [coordY, setCoordY] = useState('');

  useEffect(() => {
    api.listWorlds().then(setWorlds).catch(() => {});
  }, []);

  const buildLocationToken = () => {
    const w = worldId.trim();
    const x = coordX.trim();
    const y = coordY.trim();
    if (!x || !y) return null;
    return w ? `${w}/${x},${y}` : `${x},${y}`;
  };

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
            <div className="meta-row" style={{ justifyContent: 'space-between' }}>
              <label className="meta-label">Location</label>
              <button
                className="meta-location-helper-btn"
                onClick={() => setShowWorldHelper((v) => !v)}
                title="World/coordinate helper"
              >
                {showWorldHelper ? 'Hide helper' : 'World helper'}
              </button>
            </div>
            <input
              className="meta-input"
              value={note.location ?? ''}
              onChange={(e) => patch({ location: e.target.value || undefined })}
              placeholder='e.g. "Tokyo", "48.85,2.35", or "earth/48.85,2.35"'
            />
            {showWorldHelper && (
              <div className="meta-world-helper">
                <div className="meta-world-helper-row">
                  <label className="meta-world-helper-label">World</label>
                  <select value={worldId} onChange={(e) => setWorldId(e.target.value)}>
                    <option value="">Default (earth)</option>
                    {worlds.map((w) => <option key={w.id} value={w.id}>{w.display_name}</option>)}
                  </select>
                </div>
                <div className="meta-world-helper-row">
                  <label className="meta-world-helper-label">
                    {worldId && worlds.find((w) => w.id === worldId)?.kind === 'image' ? 'X (0–1)' : 'Latitude'}
                  </label>
                  <input value={coordX} onChange={(e) => setCoordX(e.target.value)} placeholder="0.0" />
                </div>
                <div className="meta-world-helper-row">
                  <label className="meta-world-helper-label">
                    {worldId && worlds.find((w) => w.id === worldId)?.kind === 'image' ? 'Y (0–1)' : 'Longitude'}
                  </label>
                  <input value={coordY} onChange={(e) => setCoordY(e.target.value)} placeholder="0.0" />
                </div>
                <button
                  className="meta-world-helper-apply"
                  disabled={!coordX.trim() || !coordY.trim()}
                  onClick={() => {
                    const token = buildLocationToken();
                    if (token) { patch({ location: token }); setShowWorldHelper(false); }
                  }}
                >
                  Apply
                </button>
                <p className="meta-world-helper-hint">
                  Result: <code>{buildLocationToken() ?? '(enter coordinates)'}</code>
                </p>
              </div>
            )}
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
              <label className="meta-checkbox">
                <input
                  type="checkbox"
                  checked={note.metadata.lifecycle.private ?? false}
                  onChange={(e) => patchMeta({
                    lifecycle: { ...note.metadata.lifecycle, private: e.target.checked },
                  })}
                />
                Private
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
        </div>
      )}
    </div>
  );
}
