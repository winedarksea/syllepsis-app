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

// ── Built-in styles from docs/styles.md ──────────────────────────────────────

const BUILTIN_CARDS: StyleCard[] = [
  {
    id: 'builtin:shakespearean-narrator',
    version: 1,
    name: 'Shakespearean Narrator',
    short_description:
      'A formal, dramatic, and authoritative guide who sets the scene, bridges gaps in time, and appeals directly to the audience. The tone is grand, inviting, and slightly apologetic about the limitations of the medium.',
    verbosity: 'expansive',
    perspective: 'first_person_plural',
    reading_level: 'advanced',
    voice: 'active',
    patterns: [
      { text: 'Employ iambic pentameter and end-capped rhyming couplets to elevate the prologue or epilogue.' },
      { text: 'Use grand imagery and classical allusions to establish the setting, scale, and stakes of the narrative.' },
      { text: 'Directly address the audience, frequently commanding them to use their imagination to fill in the visual gaps.' },
      { text: 'Avoid modern slang, contractions, and internal emotional disclosures.' },
    ],
    exemplars: [
      { text: 'Two households, both alike in dignity, In fair Verona, where we lay our scene, From ancient grudge break to new mutiny…' },
      { text: 'Piece out our imperfections with your thoughts; Into a thousand parts divide one man, And make imaginary puissance.' },
    ],
    source_urls: [],
  },
  {
    id: 'builtin:shakespearean-comic-sidekick',
    version: 1,
    name: 'Shakespearean Comic Sidekick',
    short_description:
      'A lively, irreverent, and quick-witted trickster or cynic who disrupts serious moments with wordplay. The tone is mocking, bawdy, playful, and highly conversational.',
    verbosity: 'expansive',
    perspective: 'first_person_singular',
    reading_level: 'advanced',
    voice: 'active',
    patterns: [
      { text: 'Rely heavily on puns, double entendres, and bawdy innuendo.' },
      { text: 'Switch fluidly between rapid-fire prose for banter and rhyming couplets for magical or mischievous incantations.' },
      { text: 'Mock the earnestness or romantic idealism of other characters using vivid, earthy metaphors.' },
      { text: 'Avoid solemnity, straightforward declarations, and passive observation.' },
    ],
    exemplars: [
      { text: 'O, then, I see Queen Mab hath been with you. She is the fairies\' midwife, and she comes in shape no bigger than an agate-stone.' },
      { text: 'Lord, what fools these mortals be!' },
    ],
    source_urls: [],
  },
  {
    id: 'builtin:shakespearean-hero',
    version: 1,
    name: 'Shakespearean Hero',
    short_description:
      'Passionate, earnest, and deeply introspective, often wrestling with heavy burdens of duty, love, or honor. The tone ranges from desperately romantic to fiercely inspirational, usually highly formal and poetic.',
    verbosity: 'expansive',
    perspective: 'first_person_soliloquy',
    reading_level: 'advanced',
    voice: 'active',
    patterns: [
      { text: 'Use sweeping soliloquies to explore internal conflict, moral dilemmas, and existential questions.' },
      { text: 'Employ extended metaphors (conceits) to describe love, war, or the human condition.' },
      { text: 'Use rhetorical questions and exclamations to convey intense emotional turmoil.' },
      { text: 'Avoid brevity, emotional detachment, and crude or lowbrow humor.' },
    ],
    exemplars: [
      { text: 'But, soft! what light through yonder window breaks? It is the east, and Juliet is the sun.' },
      { text: 'To be, or not to be, that is the question: Whether \'tis nobler in the mind to suffer the slings and arrows of outrageous fortune…' },
    ],
    source_urls: [],
  },
  {
    id: 'builtin:shakespearean-villain',
    version: 1,
    name: 'Shakespearean Villain',
    short_description:
      'Manipulative, deeply cynical, and overtly ambitious, revealing their true malicious nature only to the audience. Tone is chillingly pragmatic, deceitful, and mockingly polite to their victims.',
    verbosity: 'expansive',
    perspective: 'first_person_soliloquy',
    reading_level: 'advanced',
    voice: 'active',
    patterns: [
      { text: 'Use stark, predatory, or disease-related imagery (e.g., snakes, spiders, poison, infection).' },
      { text: 'Employ dramatic irony by outlining evil plots to the audience while feigning extreme loyalty and honesty to other characters.' },
      { text: 'Frame heinous acts as logical necessities or natural rights, justifying them with twisted logic.' },
      { text: 'Avoid genuine expressions of remorse, empathy, or hesitation.' },
    ],
    exemplars: [
      { text: 'I am not what I am.' },
      { text: 'And therefore, since I cannot prove a lover, to entertain these fair well-spoken days, I am determined to prove a villain.' },
    ],
    source_urls: [],
  },
  {
    id: 'builtin:administrative-email',
    version: 1,
    name: 'Administrative Email',
    short_description:
      'A formal, objective, and highly standardized communication used to convey policies or mandatory actions. Tone is polite, neutral, slightly bureaucratic, and completely devoid of personal emotion.',
    verbosity: 'succinct',
    perspective: 'first_person_plural',
    reading_level: 'accessible',
    voice: 'passive',
    patterns: [
      { text: 'Rely on corporate buzzwords, softened directives, and standardized greetings/sign-offs (e.g., "Please be advised", "Going forward").' },
      { text: 'Favour the passive voice to distance the sender from the policy or mandate.' },
      { text: 'Use bulleted lists for clarity and to outline specific steps or changes.' },
      { text: 'Avoid exclamation marks, slang, personal anecdotes, or any tone that could be construed as confrontational or overly enthusiastic.' },
    ],
    exemplars: [
      { text: 'Please be advised that the new expense reporting guidelines will take effect in Q3. All employees are required to submit outstanding receipts by EOD Friday.' },
      { text: 'It has come to our attention that security badges are not being worn visibly. Going forward, compliance will be strictly monitored.' },
    ],
    source_urls: [],
  },
  {
    id: 'builtin:ted-talk',
    version: 1,
    name: 'TED Talk',
    short_description:
      'An accessible, highly engaging, and intellectually stimulating presentation designed to share a "big idea." Tone is optimistic, deeply empathetic, narrative-driven, and conversational yet rehearsed.',
    verbosity: 'standard',
    perspective: 'first_person_singular',
    reading_level: 'accessible',
    voice: 'active',
    patterns: [
      { text: 'Begin with a relatable, vulnerable personal anecdote or a surprising, counter-intuitive question to hook the audience.' },
      { text: 'Transition frequently from "I" (personal experience) to "we" (shared human experience) to build a bridge of collective potential.' },
      { text: 'Translate complex data, academic research, or scientific concepts into simple, striking metaphors.' },
      { text: 'Avoid heavy academic jargon, monotone data dumping, or aggressive sales pitches.' },
    ],
    exemplars: [
      { text: 'A few years ago, I found myself sitting in my car, crying over a spreadsheet. And that\'s when I realized: everything I thought I knew about vulnerability was completely wrong.' },
      { text: 'So, what does this mean for us? It means we have the power to rewrite our cognitive scripts. Imagine a world where our failures are just data.' },
    ],
    source_urls: [],
  },
];

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
        <div className="sc-detail-title-row">
          <h2 className="sc-detail-name">{card.name}</h2>
          {builtin && <span className="sc-builtin-badge">Built-in</span>}
        </div>
        <p className="sc-detail-desc">{card.short_description}</p>
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

  const allCards = [...BUILTIN_CARDS, ...cards];
  const selectedCard = allCards.find((c) => c.id === selected) ?? null;

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
          {BUILTIN_CARDS.map((card) => (
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
          {cards.length > 0 && (
            <>
              <div className="sc-list-section-label">Your cards</div>
              {cards.map((card) => (
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

          {cards.length === 0 && (
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
