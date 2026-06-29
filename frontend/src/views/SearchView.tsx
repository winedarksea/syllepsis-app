// The search-centred web of results: a query box, collapsible filter panel, and ranked hits
// fused from exact + BM25 + vector retrieval (RRF). Selecting a hit previews its related
// notes and opens it in the editor.
// Also supports cross-book search across all tracked books.

import { useCallback, useEffect, useRef, useState } from 'react';
import { api } from '../lib/api';
import { displayTitle } from '../lib/utils';
import { useStore } from '../lib/store';
import { PageHeader } from '../components/PageHeader';
import type { SearchResults, CrossBookNote, ObjectType, SearchFilter, NoteVisibility } from '../types';
import { RelatedCarousel } from '../components/RelatedCarousel';
import { formatSearchRelevancePercent } from '../lib/searchRelevance';
import './SearchView.css';

const ALL_OBJECT_TYPES: ObjectType[] = [
  'note', 'quote', 'reference', 'todo', 'qa', 'table', 'picture', 'drawing', 'code',
];

const FRESHNESS_PRESETS: { label: string; days: number | null }[] = [
  { label: 'Any time', days: null },
  { label: 'Today', days: 1 },
  { label: '7 days', days: 7 },
  { label: '30 days', days: 30 },
  { label: 'This year', days: 365 },
];

const LENGTH_PRESETS: { label: string; min: number | null; max: number | null }[] = [
  { label: 'Any length', min: null, max: null },
  { label: 'Short (< 200)', min: null, max: 200 },
  { label: 'Medium (200–1000)', min: 200, max: 1000 },
  { label: 'Long (> 1000)', min: 1000, max: null },
];

function activeFilterCount(
  categories: string[],
  freshnessIndex: number,
  lengthIndex: number,
  objectTypes: ObjectType[],
  starredOnly: boolean,
  allBooks: boolean,
  visibility: NoteVisibility,
): number {
  return (
    (categories.length > 0 ? 1 : 0) +
    (freshnessIndex > 0 ? 1 : 0) +
    (lengthIndex > 0 ? 1 : 0) +
    (objectTypes.length > 0 ? 1 : 0) +
    (starredOnly ? 1 : 0) +
    (allBooks ? 1 : 0) +
    (visibility !== 'active' ? 1 : 0)
  );
}

function buildFilter(
  categories: string[],
  freshnessIndex: number,
  lengthIndex: number,
  objectTypes: ObjectType[],
  starredOnly: boolean,
  visibility: NoteVisibility,
): SearchFilter {
  const preset = FRESHNESS_PRESETS[freshnessIndex];
  const lenPreset = LENGTH_PRESETS[lengthIndex];
  return {
    visibility,
    categories,
    updated_after: preset.days
      ? new Date(Date.now() - preset.days * 24 * 3600 * 1000).toISOString()
      : null,
    min_body_len: lenPreset.min,
    max_body_len: lenPreset.max,
    object_types: objectTypes,
    starred_only: starredOnly,
  };
}

function relativeDate(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const days = Math.floor(diff / 86400000);
  if (days === 0) return 'today';
  if (days === 1) return '1d ago';
  if (days < 30) return `${days}d ago`;
  if (days < 365) return `${Math.floor(days / 30)}mo ago`;
  return `${Math.floor(days / 365)}y ago`;
}

const WINDOW_SIZE = 15;

export function SearchView() {
  const { openEditor } = useStore();

  // Query
  const [query, setQuery] = useState('');

  // Filter panel state
  const [panelOpen, setPanelOpen] = useState(false);
  const [allBooks, setAllBooks] = useState(false);
  const [categories, setCategories] = useState<string[]>([]);
  const [freshnessIndex, setFreshnessIndex] = useState(0);
  const [lengthIndex, setLengthIndex] = useState(0);
  const [objectTypes, setObjectTypes] = useState<ObjectType[]>([]);
  const [starredOnly, setStarredOnly] = useState(false);
  const [visibility, setVisibility] = useState<NoteVisibility>('active');
  const [showAllFacets, setShowAllFacets] = useState(false);

  // Results
  const [results, setResults] = useState<SearchResults | null>(null);
  const [crossBookResults, setCrossBookResults] = useState<CrossBookNote[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<string | null>(null);

  // Windowing
  const [visibleCount, setVisibleCount] = useState(WINDOW_SIZE);
  const sentinelRef = useRef<HTMLDivElement | null>(null);
  const observerRef = useRef<IntersectionObserver | null>(null);

  const debounce = useRef<ReturnType<typeof setTimeout> | null>(null);

  const numActive = activeFilterCount(categories, freshnessIndex, lengthIndex, objectTypes, starredOnly, allBooks, visibility);

  const run = useCallback((
    q: string,
    cats: string[],
    freshIdx: number,
    lenIdx: number,
    types: ObjectType[],
    starred: boolean,
    crossBook: boolean,
    lifecycleVisibility: NoteVisibility,
  ) => {
    if (!q.trim()) {
      setResults(null);
      setCrossBookResults(null);
      return;
    }
    setLoading(true);
    setError(null);
    setVisibleCount(WINDOW_SIZE);
    if (crossBook) {
      api.searchAcrossBooks(q)
        .then((r) => { setCrossBookResults(r); setResults(null); })
        .catch((e) => setError(String(e)))
        .finally(() => setLoading(false));
    } else {
      const filter = buildFilter(cats, freshIdx, lenIdx, types, starred, lifecycleVisibility);
      api.search(q, filter)
        .then((r) => { setResults(r); setCrossBookResults(null); setError(null); })
        .catch((e) => setError(String(e)))
        .finally(() => setLoading(false));
    }
  }, []);

  // Debounced search on any filter/query change
  useEffect(() => {
    if (debounce.current) clearTimeout(debounce.current);
    debounce.current = setTimeout(
      () => run(query, categories, freshnessIndex, lengthIndex, objectTypes, starredOnly, allBooks, visibility),
      300,
    );
    return () => { if (debounce.current) clearTimeout(debounce.current); };
  }, [query, categories, freshnessIndex, lengthIndex, objectTypes, starredOnly, allBooks, visibility, run]);

  // IntersectionObserver for windowed reveal
  useEffect(() => {
    if (observerRef.current) observerRef.current.disconnect();
    const hits = results?.hits ?? [];
    if (visibleCount >= hits.length) return;
    observerRef.current = new IntersectionObserver(
      (entries) => {
        if (entries[0].isIntersecting) {
          setVisibleCount((c) => Math.min(c + WINDOW_SIZE, hits.length));
        }
      },
      { threshold: 0.1 },
    );
    if (sentinelRef.current) observerRef.current.observe(sentinelRef.current);
    return () => observerRef.current?.disconnect();
  }, [results, visibleCount]);

  const toggleCategory = useCallback((cat: string) => {
    setCategories((prev) =>
      prev.includes(cat) ? prev.filter((c) => c !== cat) : [...prev, cat],
    );
  }, []);

  const toggleObjectType = useCallback((type: ObjectType) => {
    setObjectTypes((prev) =>
      prev.includes(type) ? prev.filter((t) => t !== type) : [...prev, type],
    );
  }, []);

  const clearAll = useCallback(() => {
    setCategories([]);
    setFreshnessIndex(0);
    setLengthIndex(0);
    setObjectTypes([]);
    setStarredOnly(false);
    setVisibility('active');
    setAllBooks(false);
  }, []);

  function scoreTooltip(hit: SearchResults['hits'][0]): string {
    const s = hit.ranking_signals;
    return (
      `Relevance ${formatSearchRelevancePercent(hit)} | ` +
      `RRF total ${s.total.toFixed(3)} = ` +
      `exact ${s.exact.toFixed(3)} + bm25 ${s.bm25.toFixed(3)} + vector ${s.vector.toFixed(3)}` +
      (s.vector_similarity > 0 ? ` | cos ${s.vector_similarity.toFixed(3)}` : '')
    );
  }

  const visibleFacets = results?.facets ?? [];
  const FACET_THRESHOLD = 8;
  const displayedFacets = showAllFacets ? visibleFacets : visibleFacets.slice(0, FACET_THRESHOLD);

  const hits = results?.hits ?? [];
  const shownHits = hits.slice(0, visibleCount);

  return (
    <div className="sv-root">
      {/* ── Header ── */}
      <PageHeader>
        <input
          className="sv-input"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={allBooks ? 'Search across all tracked books…' : 'Search your book — exact, keyword, and meaning…'}
          autoFocus
        />
        <button
          className={`sv-filter-toggle ${panelOpen ? 'active' : ''}`}
          onClick={() => setPanelOpen((v) => !v)}
          title="Toggle search filters"
        >
          Filters{numActive > 0 ? ` (${numActive})` : ''}
        </button>
      </PageHeader>

      {/* ── Collapsible filter panel ── */}
      {panelOpen && (
        <div className="sv-filter-panel">
          <div className="sv-filter-row">
            <label className="sv-filter-label">Visibility</label>
            <select
              className="sv-filter-select"
              value={visibility}
              onChange={(e) => setVisibility(e.target.value as NoteVisibility)}
              disabled={allBooks}
            >
              <option value="active">Active</option>
              <option value="archived">Archived</option>
              <option value="trash">Trash</option>
            </select>
          </div>

          <div className="sv-filter-row">
            <label className="sv-filter-label">Updated</label>
            <select
              className="sv-filter-select"
              value={freshnessIndex}
              onChange={(e) => setFreshnessIndex(Number(e.target.value))}
            >
              {FRESHNESS_PRESETS.map((p, i) => (
                <option key={p.label} value={i}>{p.label}</option>
              ))}
            </select>
          </div>

          <div className="sv-filter-row">
            <label className="sv-filter-label">Length</label>
            <select
              className="sv-filter-select"
              value={lengthIndex}
              onChange={(e) => setLengthIndex(Number(e.target.value))}
            >
              {LENGTH_PRESETS.map((p, i) => (
                <option key={p.label} value={i}>{p.label}</option>
              ))}
            </select>
          </div>

          <div className="sv-filter-row sv-filter-row--top">
            <label className="sv-filter-label">Type</label>
            <div className="sv-filter-checkgroup">
              {ALL_OBJECT_TYPES.map((t) => (
                <label key={t} className="sv-filter-check">
                  <input
                    type="checkbox"
                    checked={objectTypes.includes(t)}
                    onChange={() => toggleObjectType(t)}
                  />
                  {t}
                </label>
              ))}
            </div>
          </div>

          <div className="sv-filter-row">
            <label className="sv-filter-check sv-filter-label">
              <input
                type="checkbox"
                checked={starredOnly}
                onChange={(e) => setStarredOnly(e.target.checked)}
              />
              Starred only
            </label>
          </div>

          <div className="sv-filter-row">
            <label className="sv-filter-check sv-filter-label">
              <input
                type="checkbox"
                checked={allBooks}
                onChange={(e) => setAllBooks(e.target.checked)}
              />
              Search all books
            </label>
          </div>

          {/* Category facets inside the panel */}
          {!allBooks && visibleFacets.length > 0 && (
            <div className="sv-filter-row sv-filter-row--top">
              <label className="sv-filter-label">Categories</label>
              <div className="sv-facets-inline">
                {displayedFacets.map((f) => (
                  <button
                    key={f.category}
                    className={`sv-facet ${categories.includes(f.category) ? 'active' : ''}`}
                    onClick={() => toggleCategory(f.category)}
                  >
                    #{f.category} <span className="sv-facet-count">{f.count}</span>
                  </button>
                ))}
                {visibleFacets.length > FACET_THRESHOLD && (
                  <button className="sv-facets-more" onClick={() => setShowAllFacets((v) => !v)}>
                    {showAllFacets ? 'Show less' : `+${visibleFacets.length - FACET_THRESHOLD} more`}
                  </button>
                )}
              </div>
            </div>
          )}

          {numActive > 0 && (
            <div className="sv-filter-row sv-filter-actions">
              <button className="sv-filter-clear" onClick={clearAll}>Clear all filters</button>
            </div>
          )}
        </div>
      )}

      {/* ── Active filter chips ── */}
      {numActive > 0 && !panelOpen && (
        <div className="sv-active-chips">
          {categories.map((c) => (
            <button key={c} className="sv-chip" onClick={() => toggleCategory(c)}>
              #{c} ×
            </button>
          ))}
          {freshnessIndex > 0 && (
            <button className="sv-chip" onClick={() => setFreshnessIndex(0)}>
              {FRESHNESS_PRESETS[freshnessIndex].label} ×
            </button>
          )}
          {lengthIndex > 0 && (
            <button className="sv-chip" onClick={() => setLengthIndex(0)}>
              {LENGTH_PRESETS[lengthIndex].label} ×
            </button>
          )}
          {objectTypes.map((t) => (
            <button key={t} className="sv-chip" onClick={() => toggleObjectType(t)}>
              {t} ×
            </button>
          ))}
          {starredOnly && (
            <button className="sv-chip" onClick={() => setStarredOnly(false)}>★ starred ×</button>
          )}
          {allBooks && (
            <button className="sv-chip" onClick={() => setAllBooks(false)}>all books ×</button>
          )}
        </div>
      )}

      <div className="sv-body">
        {error && <div className="sv-state sv-error">{error}</div>}
        {!query.trim() && (
          <div className="sv-state sv-hint">
            {allBooks
              ? 'Search all your tracked books to find notes and create cross-book links.'
              : 'Type to search across every note.'}
          </div>
        )}
        {loading && <div className="sv-state">Searching…</div>}

        {/* Current-book results */}
        {!allBooks && results && hits.length === 0 && query.trim() && !loading && (
          <div className="sv-state">No matches.</div>
        )}
        {!allBooks && hits.length > 0 && (
          <div className="sv-results">
            {shownHits.map((hit) => (
              <div
                key={hit.note_id}
                className={`sv-hit ${preview === hit.note_id ? 'selected' : ''}`}
                onClick={() => setPreview(hit.note_id)}
                onDoubleClick={() => openEditor(hit.note_id)}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => e.key === 'Enter' && openEditor(hit.note_id)}
              >
                <div className="sv-hit-header">
                  <span className="sv-hit-title">{displayTitle(hit.title, hit.summary)}</span>
                  <div className="sv-hit-meta">
                    {hit.starred && <span className="sv-hit-star" title="Starred">★</span>}
                    {hit.archived && <span className="sv-hit-type">archived</span>}
                    {hit.marked_for_deletion_at && <span className="sv-hit-type">trash</span>}
                    {hit.status && <span className="sv-hit-type">{hit.status.replace(/_/g, ' ')}</span>}
                    <span className="sv-hit-type">{hit.object_type}</span>
                    <span className="sv-hit-date">{relativeDate(hit.updated)}</span>
                    <span
                      className="sv-hit-score"
                      title={scoreTooltip(hit)}
                      aria-label="Ranking score details"
                    >
                      {formatSearchRelevancePercent(hit)}
                    </span>
                    <button
                      className="sv-hit-open"
                      onClick={(e) => { e.stopPropagation(); openEditor(hit.note_id); }}
                    >
                      Open
                    </button>
                  </div>
                </div>
                {hit.summary && <p className="sv-hit-summary">{hit.summary}</p>}
                {hit.snippet && <p className="sv-hit-snippet">{hit.snippet}</p>}
                {hit.categories.length > 0 && (
                  <div className="sv-hit-tags">
                    {hit.categories.map((c) => <span key={c} className="sv-tag">#{c}</span>)}
                  </div>
                )}
              </div>
            ))}
            {/* Intersection sentinel for windowed reveal */}
            {visibleCount < hits.length && (
              <div ref={sentinelRef} className="sv-sentinel" aria-hidden="true" />
            )}
          </div>
        )}

        {/* Cross-book results */}
        {allBooks && crossBookResults !== null && crossBookResults.length === 0 && query.trim() && !loading && (
          <div className="sv-state">No matches in other books.</div>
        )}
        {allBooks && crossBookResults && crossBookResults.length > 0 && (
          <div className="sv-results">
            {crossBookResults.map((hit) => (
              <div key={`${hit.book_path}/${hit.note_id}`} className="sv-hit sv-hit-cross-book">
                <div className="sv-hit-header">
                  <span className="sv-hit-title">{displayTitle(hit.title, hit.summary)}</span>
                  <span className="sv-cross-book-badge">{hit.book_name}</span>
                </div>
                {hit.summary && <p className="sv-hit-summary">{hit.summary}</p>}
                <div className="sv-cross-book-link-hint">
                  Link syntax: <code>[{displayTitle(hit.title, hit.summary)}](book:{hit.book_name}/{hit.note_id})</code>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {!allBooks && preview && <RelatedCarousel noteId={preview} />}
    </div>
  );
}
