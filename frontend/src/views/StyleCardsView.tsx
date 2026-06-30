// Style cards: capture writing style for LLM-driven rewrites and style grading.

import { useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api';
import type { StyleCard, StyleVerbosity, StylePerspective, StyleReadingLevel, StyleVoice } from '../types';
import './StyleCardsView.css';

const VERBOSITIES: StyleVerbosity[] = ['succinct', 'standard', 'expansive'];
const PERSPECTIVES: StylePerspective[] = [
  'first_person_singular',
  'first_person_plural',
  'first_person_soliloquy',
  'second_person',
  'third_person_objective',
  'third_person_omniscient',
  'third_person_limited',
];
const READING_LEVELS: StyleReadingLevel[] = ['elementary', 'accessible', 'advanced', 'expert'];
const VOICES: StyleVoice[] = ['active', 'passive'];

function humanize(s: string): string {
  return s.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

// ── Built-in card id sentinel ─────────────────────────────────────────────────

const isBuiltin = (id: string) => id.startsWith('builtin:');


// ── Blank card for new user cards ─────────────────────────────────────────────

const blankCard = (): StyleCard => ({
  id: '',
  version: 1,
  name: '',
  short_description: '',
  verbosity: 'standard',
  perspective: 'first_person_singular',
  reading_level: 'accessible',
  voice: 'active',
  patterns: [],
  exemplars: [],
  source_urls: [],
});

// ── Editor ────────────────────────────────────────────────────────────────────

interface CardEditorProps {
  initial: StyleCard;
  onSave: (card: StyleCard) => void;
  onCancel: () => void;
}

function CardEditor({ initial, onSave, onCancel }: CardEditorProps) {
  const [card, setCard] = useState<StyleCard>(initial);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const patch = (partial: Partial<StyleCard>) => setCard((c) => ({ ...c, ...partial }));

  const addPattern = () => patch({ patterns: [...card.patterns, { text: '' }] });
  const updatePattern = (i: number, text: string) =>
    patch({ patterns: card.patterns.map((p, j) => (j === i ? { text } : p)) });
  const removePattern = (i: number) =>
    patch({ patterns: card.patterns.filter((_, j) => j !== i) });

  const addExemplar = () => patch({ exemplars: [...card.exemplars, { text: '' }] });
  const updateExemplar = (i: number, text: string) =>
    patch({ exemplars: card.exemplars.map((e, j) => (j === i ? { text } : e)) });
  const removeExemplar = (i: number) =>
    patch({ exemplars: card.exemplars.filter((_, j) => j !== i) });

  const submit = async () => {
    if (!card.name.trim()) { setError('Name is required.'); return; }
    setBusy(true);
    setError(null);
    try {
      const saved = await api.saveStyleCard(card);
      onSave(saved);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="sc-editor">
      <h3 className="sc-editor-title">{card.id ? 'Edit style card' : 'New style card'}</h3>
      {error && <div className="sc-error" onClick={() => setError(null)}>{error}</div>}

      <label className="sc-field">
        <span>Name</span>
        <input
          value={card.name}
          onChange={(e) => patch({ name: e.target.value })}
          placeholder="e.g., Shakespearean Narrator, TED Talk, Administrative Email"
        />
      </label>

      <label className="sc-field">
        <span>Short description</span>
        <textarea
          value={card.short_description}
          onChange={(e) => patch({ short_description: e.target.value })}
          rows={2}
          placeholder="Describe the overall tone (e.g., enthusiastic, cynical, objective) and vocabulary level."
        />
      </label>

      <div className="sc-attrs-grid">
        <label className="sc-field">
          <span>Verbosity</span>
          <select value={card.verbosity} onChange={(e) => patch({ verbosity: e.target.value as StyleVerbosity })}>
            {VERBOSITIES.map((v) => <option key={v} value={v}>{humanize(v)}</option>)}
          </select>
        </label>
        <label className="sc-field">
          <span>Perspective</span>
          <select value={card.perspective} onChange={(e) => patch({ perspective: e.target.value as StylePerspective })}>
            {PERSPECTIVES.map((p) => <option key={p} value={p}>{humanize(p)}</option>)}
          </select>
        </label>
        <label className="sc-field">
          <span>Reading level</span>
          <select value={card.reading_level} onChange={(e) => patch({ reading_level: e.target.value as StyleReadingLevel })}>
            {READING_LEVELS.map((r) => <option key={r} value={r}>{humanize(r)}</option>)}
          </select>
        </label>
        <label className="sc-field">
          <span>Voice</span>
          <select value={card.voice} onChange={(e) => patch({ voice: e.target.value as StyleVoice })}>
            {VOICES.map((v) => <option key={v} value={v}>{humanize(v)}</option>)}
          </select>
        </label>
      </div>

      <div className="sc-exemplars">
        <div className="sc-subhead">
          Patterns
          <button className="sc-add-exemplar-btn" onClick={addPattern}>+ Add</button>
        </div>
        {card.patterns.map((p, i) => (
          <div key={i} className="sc-exemplar-row">
            <textarea
              className="sc-pattern-input"
              value={p.text}
              onChange={(e) => updatePattern(i, e.target.value)}
              rows={2}
              placeholder="Describe a recurring style element or anti-pattern…"
            />
            <button className="sc-remove-btn" onClick={() => removePattern(i)} title="Remove">×</button>
          </div>
        ))}
      </div>

      <div className="sc-exemplars">
        <div className="sc-subhead">
          Exemplars
          <button className="sc-add-exemplar-btn" onClick={addExemplar}>+ Add</button>
        </div>
        {card.exemplars.map((ex, i) => (
          <div key={i} className="sc-exemplar-row">
            <textarea
              className="sc-pattern-input"
              value={ex.text}
              onChange={(e) => updateExemplar(i, e.target.value)}
              rows={2}
              placeholder="1–3 sentence snippet…"
            />
            <button className="sc-remove-btn" onClick={() => removeExemplar(i)} title="Remove">×</button>
          </div>
        ))}
      </div>

      <div className="sc-editor-actions">
        <button className="sc-btn" onClick={onCancel} disabled={busy}>Cancel</button>
        <button className="sc-btn sc-btn-primary" onClick={submit} disabled={busy}>
          {busy ? 'Saving…' : 'Save'}
        </button>
      </div>
    </div>
  );
}

// ── Detail ────────────────────────────────────────────────────────────────────

function CardDetail({
  card,
  onEdit,
  onDelete,
  onDuplicate,
}: {
  card: StyleCard;
  onEdit?: () => void;
  onDelete?: () => void;
  onDuplicate?: () => void;
}) {
  const builtin = isBuiltin(card.id);
  return (
    <div className="sc-detail">
      <div className="sc-detail-header">
        <div className="sc-detail-header-top">
          <div className="sc-detail-title-row">
            <h2 className="sc-detail-name">{card.name}</h2>
            {builtin && <span className="sc-builtin-badge">Built-in</span>}
          </div>
          <div className="sc-detail-actions">
            {builtin ? (
              <button className="sc-btn" onClick={onDuplicate}>Duplicate</button>
            ) : (
              <>
                <button className="sc-btn" onClick={onEdit}>Edit</button>
                <button className="sc-btn sc-btn-danger" onClick={onDelete}>Delete</button>
              </>
            )}
          </div>
        </div>
        <p className="sc-detail-desc">{card.short_description}</p>
      </div>

      <div className="sc-attrs-grid sc-detail-attrs">
        {([
          ['Verbosity', card.verbosity],
          ['Perspective', card.perspective],
          ['Reading level', card.reading_level],
          ['Voice', card.voice],
        ] as [string, string][]).map(([label, value]) => (
          <div key={label} className="sc-attr">
            <span className="sc-attr-label">{label}</span>
            <span className="sc-attr-value">{humanize(value)}</span>
          </div>
        ))}
      </div>

      {card.patterns.length > 0 && (
        <div className="sc-exemplars-display">
          <div className="sc-subhead sc-subhead-plain">Patterns</div>
          <ul className="sc-patterns-list">
            {card.patterns.map((p, i) => (
              <li key={i} className="sc-pattern-item">{p.text}</li>
            ))}
          </ul>
        </div>
      )}

      {card.exemplars.length > 0 && (
        <div className="sc-exemplars-display">
          <div className="sc-subhead sc-subhead-plain">Exemplars</div>
          {card.exemplars.map((ex, i) => (
            <blockquote key={i} className="sc-exemplar-text">"{ex.text}"</blockquote>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Root view ─────────────────────────────────────────────────────────────────

export function StyleCardsView() {
  const [cards, setCards] = useState<StyleCard[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<StyleCard | null>(null);
  const [selected, setSelected] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setCards(await api.listStyleCards());
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const handleSave = useCallback(async (saved: StyleCard) => {
    await load();
    setEditing(null);
    setSelected(saved.id);
  }, [load]);

  const handleDelete = useCallback(async (id: string) => {
    if (!window.confirm('Delete this style card?')) return;
    try {
      await api.deleteStyleCard(id);
      setCards((prev) => prev.filter((c) => c.id !== id));
      if (selected === id) setSelected(null);
    } catch (e) {
      setError(String(e));
    }
  }, [selected]);

  const handleDuplicate = useCallback((card: StyleCard) => {
    const copy = { ...card, id: '', name: `${card.name} (copy)` };
    setEditing(copy);
    setSelected(null);
  }, []);

  const builtinCards = cards.filter((c) => isBuiltin(c.id));
  const userCards = cards.filter((c) => !isBuiltin(c.id));
  const selectedCard = cards.find((c) => c.id === selected) ?? null;

  if (loading) return <div className="sc-state">Loading style cards…</div>;

  return (
    <div className="sc-root">
      {/* Left: card list */}
      <div className="sc-sidebar">
        <div className="sc-sidebar-header">
          <h2 className="sc-sidebar-title">Style Cards</h2>
          <button className="sc-new-btn" onClick={() => { setEditing(blankCard()); setSelected(null); }}>
            + New
          </button>
        </div>
        {error && <div className="sc-error" onClick={() => setError(null)}>{error}</div>}

        <nav className="sc-list">
          {/* Built-in section */}
          <div className="sc-list-section-label">Built-in</div>
          {builtinCards.map((card) => (
            <button
              key={card.id}
              className={`sc-list-item ${selected === card.id ? 'active' : ''}`}
              onClick={() => { setSelected(card.id); setEditing(null); }}
            >
              <div className="sc-list-item-title">{card.name}</div>
              <div className="sc-list-item-attrs">
                {humanize(card.verbosity)} · {humanize(card.reading_level)}
              </div>
            </button>
          ))}

          {/* User cards section */}
          {userCards.length > 0 && (
            <>
              <div className="sc-list-section-label">Your cards</div>
              {userCards.map((card) => (
                <button
                  key={card.id}
                  className={`sc-list-item ${selected === card.id ? 'active' : ''}`}
                  onClick={() => { setSelected(card.id); setEditing(null); }}
                >
                  <div className="sc-list-item-title">{card.name || '(no name)'}</div>
                  <div className="sc-list-item-attrs">
                    {humanize(card.verbosity)} · {humanize(card.reading_level)}
                  </div>
                </button>
              ))}
            </>
          )}

          {userCards.length === 0 && (
            <div className="sc-empty">No custom cards yet. Create one or duplicate a built-in.</div>
          )}
        </nav>
      </div>

      {/* Right: editor or detail */}
      <div className="sc-main">
        {editing ? (
          <CardEditor
            initial={editing}
            onSave={handleSave}
            onCancel={() => setEditing(null)}
          />
        ) : selectedCard ? (
          <CardDetail
            card={selectedCard}
            onEdit={() => setEditing(selectedCard)}
            onDelete={() => handleDelete(selectedCard.id)}
            onDuplicate={() => handleDuplicate(selectedCard)}
          />
        ) : (
          <div className="sc-placeholder">
            <p>Select a style card or create a new one.</p>
            <p className="sc-placeholder-hint">
              Style cards capture writing style attributes so the LLM can rewrite or grade notes in that style.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
