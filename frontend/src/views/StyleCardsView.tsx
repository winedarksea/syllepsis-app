// Style cards: capture writing style for LLM-driven rewrites and style grading.
// Each card defines field, tenor, mode, density, texture, organization, and example snippets.

import { useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api';
import type { StyleCard, StyleField, StyleTenor, StyleMode, StyleDensity, StyleTexture, StyleOrganization, StyleExemplar } from '../types';
import './StyleCardsView.css';

const FIELDS: StyleField[] = ['technical', 'instructional', 'persuasive', 'narrative', 'reflective', 'administrative'];
const TENORS: StyleTenor[] = ['intimate', 'peer', 'expert_to_peer', 'expert_to_novice', 'institutional'];
const MODES: StyleMode[] = ['spoken', 'conversational_written', 'edited_written', 'formal_written'];
const DENSITIES: StyleDensity[] = ['sparse', 'moderate', 'dense'];
const TEXTURES: StyleTexture[] = ['plain', 'polished', 'vivid', 'aphoristic', 'procedural'];
const ORGANIZATIONS: StyleOrganization[] = ['conclusion_first', 'stepwise', 'narrative', 'compare_contrast', 'problem_solution'];

function humanize(s: string): string {
  return s.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

const blankCard = (): StyleCard => ({
  id: '',
  version: 1,
  short_description: '',
  field: 'reflective',
  tenor: 'peer',
  mode: 'edited_written',
  density: 'moderate',
  texture: 'plain',
  organization: 'conclusion_first',
  exemplars: [],
  source_urls: [],
});

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

  const addExemplar = () =>
    patch({ exemplars: [...card.exemplars, { text: '', note: '' }] });

  const updateExemplar = (i: number, ex: StyleExemplar) =>
    patch({ exemplars: card.exemplars.map((e, j) => (j === i ? ex : e)) });

  const removeExemplar = (i: number) =>
    patch({ exemplars: card.exemplars.filter((_, j) => j !== i) });

  const submit = async () => {
    if (!card.short_description.trim()) { setError('Description is required.'); return; }
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
        <span>Short description</span>
        <textarea
          value={card.short_description}
          onChange={(e) => patch({ short_description: e.target.value })}
          rows={2}
          placeholder="A sentence or two describing the style…"
        />
      </label>

      <div className="sc-attrs-grid">
        <label className="sc-field">
          <span>Field</span>
          <select value={card.field} onChange={(e) => patch({ field: e.target.value as StyleField })}>
            {FIELDS.map((f) => <option key={f} value={f}>{humanize(f)}</option>)}
          </select>
        </label>
        <label className="sc-field">
          <span>Tenor</span>
          <select value={card.tenor} onChange={(e) => patch({ tenor: e.target.value as StyleTenor })}>
            {TENORS.map((t) => <option key={t} value={t}>{humanize(t)}</option>)}
          </select>
        </label>
        <label className="sc-field">
          <span>Mode</span>
          <select value={card.mode} onChange={(e) => patch({ mode: e.target.value as StyleMode })}>
            {MODES.map((m) => <option key={m} value={m}>{humanize(m)}</option>)}
          </select>
        </label>
        <label className="sc-field">
          <span>Density</span>
          <select value={card.density} onChange={(e) => patch({ density: e.target.value as StyleDensity })}>
            {DENSITIES.map((d) => <option key={d} value={d}>{humanize(d)}</option>)}
          </select>
        </label>
        <label className="sc-field">
          <span>Texture</span>
          <select value={card.texture} onChange={(e) => patch({ texture: e.target.value as StyleTexture })}>
            {TEXTURES.map((t) => <option key={t} value={t}>{humanize(t)}</option>)}
          </select>
        </label>
        <label className="sc-field">
          <span>Organization</span>
          <select value={card.organization} onChange={(e) => patch({ organization: e.target.value as StyleOrganization })}>
            {ORGANIZATIONS.map((o) => <option key={o} value={o}>{humanize(o)}</option>)}
          </select>
        </label>
      </div>

      <div className="sc-exemplars">
        <div className="sc-subhead">
          Exemplars
          <button className="sc-add-exemplar-btn" onClick={addExemplar}>+ Add</button>
        </div>
        {card.exemplars.map((ex, i) => (
          <div key={i} className="sc-exemplar-row">
            <div className="sc-exemplar-fields">
              <textarea
                value={ex.text}
                onChange={(e) => updateExemplar(i, { ...ex, text: e.target.value })}
                rows={2}
                placeholder="1–3 sentence snippet…"
              />
              <input
                value={ex.note}
                onChange={(e) => updateExemplar(i, { ...ex, note: e.target.value })}
                placeholder="What this snippet demonstrates…"
              />
            </div>
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

function CardDetail({ card, onEdit, onDelete }: { card: StyleCard; onEdit: () => void; onDelete: () => void }) {
  return (
    <div className="sc-detail">
      <div className="sc-detail-header">
        <p className="sc-detail-desc">{card.short_description}</p>
        <div className="sc-detail-actions">
          <button className="sc-btn" onClick={onEdit}>Edit</button>
          <button className="sc-btn sc-btn-danger" onClick={onDelete}>Delete</button>
        </div>
      </div>
      <div className="sc-attrs-grid sc-detail-attrs">
        {[
          ['Field', card.field],
          ['Tenor', card.tenor],
          ['Mode', card.mode],
          ['Density', card.density],
          ['Texture', card.texture],
          ['Organization', card.organization],
        ].map(([label, value]) => (
          <div key={label} className="sc-attr">
            <span className="sc-attr-label">{label}</span>
            <span className="sc-attr-value">{humanize(value)}</span>
          </div>
        ))}
      </div>
      {card.exemplars.length > 0 && (
        <div className="sc-exemplars-display">
          <div className="sc-subhead">Exemplars</div>
          {card.exemplars.map((ex, i) => (
            <div key={i} className="sc-exemplar-display">
              <blockquote className="sc-exemplar-text">"{ex.text}"</blockquote>
              {ex.note && <p className="sc-exemplar-note">{ex.note}</p>}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

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
        {cards.length === 0 ? (
          <div className="sc-empty">No style cards yet. Create one to capture a writing style.</div>
        ) : (
          <nav className="sc-list">
            {cards.map((card) => (
              <button
                key={card.id}
                className={`sc-list-item ${selected === card.id ? 'active' : ''}`}
                onClick={() => { setSelected(card.id); setEditing(null); }}
              >
                <div className="sc-list-item-title">{card.short_description || '(no description)'}</div>
                <div className="sc-list-item-attrs">
                  {humanize(card.field)} · {humanize(card.mode)}
                </div>
              </button>
            ))}
          </nav>
        )}
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
