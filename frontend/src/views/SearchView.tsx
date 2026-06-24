// The search-centred web of results: a query box, category facet filters, and ranked hits
// fused from exact + BM25 + vector retrieval (RRF). Selecting a hit previews its related
// notes and opens it in the editor.
// Also supports cross-book search across all tracked books.

import { useCallback, useEffect, useRef, useState } from 'react';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import type { SearchResults, CrossBookNote } from '../types';
import { RelatedCarousel } from '../components/RelatedCarousel';
import './SearchView.css';

function formatScore(score: number): string {
  return score.toFixed(3);
}

export function SearchView() {
  const { openEditor } = useStore();
  const [query, setQuery] = useState('');
  const [activeFacets, setActiveFacets] = useState<string[]>([]);
  const [results, setResults] = useState<SearchResults | null>(null);
  const [crossBookResults, setCrossBookResults] = useState<CrossBookNote[] | null>(null);
  const [crossBookMode, setCrossBookMode] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const debounce = useRef<ReturnType<typeof setTimeout> | null>(null);

  const run = useCallback((q: string, facets: string[], crossBook: boolean) => {
    if (!q.trim()) { setResults(null); setCrossBookResults(null); return; }
    setLoading(true);
    setError(null);
    if (crossBook) {
      api.searchAcrossBooks(q)
        .then((r) => { setCrossBookResults(r); setResults(null); })
        .catch((e) => setError(String(e)))
        .finally(() => setLoading(false));
    } else {
      api.search(q, facets)
        .then((r) => { setResults(r); setCrossBookResults(null); setError(null); })
        .catch((e) => setError(String(e)))
        .finally(() => setLoading(false));
    }
  }, []);

  // Debounced search-as-you-type.
  useEffect(() => {
    if (debounce.current) clearTimeout(debounce.current);
    debounce.current = setTimeout(() => run(query, activeFacets, crossBookMode), 300);
    return () => { if (debounce.current) clearTimeout(debounce.current); };
  }, [query, activeFacets, crossBookMode, run]);

  const toggleFacet = useCallback((cat: string) => {
    setActiveFacets((prev) =>
      prev.includes(cat) ? prev.filter((c) => c !== cat) : [...prev, cat],
    );
  }, []);

  return (
    <div className="sv-root">
      <div className="sv-header">
        <input
          className="sv-input"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={crossBookMode ? 'Search across all tracked books…' : 'Search your book — exact, keyword, and meaning…'}
          autoFocus
        />
        <button
          className={`sv-cross-book-toggle ${crossBookMode ? 'active' : ''}`}
          onClick={() => { setCrossBookMode((v) => !v); setResults(null); setCrossBookResults(null); }}
          title="Search across all tracked books"
        >
          All books
        </button>
      </div>

      {!crossBookMode && results && results.facets.length > 0 && (
        <div className="sv-facets">
          {results.facets.map((f) => (
            <button
              key={f.category}
              className={`sv-facet ${activeFacets.includes(f.category) ? 'active' : ''}`}
              onClick={() => toggleFacet(f.category)}
            >
              #{f.category} <span className="sv-facet-count">{f.count}</span>
            </button>
          ))}
        </div>
      )}

      <div className="sv-body">
        {error && <div className="sv-state sv-error">{error}</div>}
        {!query.trim() && (
          <div className="sv-state sv-hint">
            {crossBookMode
              ? 'Search all your tracked books to find notes and create cross-book links.'
              : 'Type to search across every note.'}
          </div>
        )}
        {loading && <div className="sv-state">Searching…</div>}

        {/* Current-book results */}
        {!crossBookMode && results && results.hits.length === 0 && query.trim() && !loading && (
          <div className="sv-state">No matches.</div>
        )}
        {!crossBookMode && results && results.hits.length > 0 && (
          <div className="sv-results">
            {results.hits.map((hit) => (
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
                  <span className="sv-hit-title">{hit.title || '(untitled)'}</span>
                  <div className="sv-hit-meta">
                    <span
                      className="sv-hit-score"
                      title={`RRF total ${formatScore(hit.ranking_signals.total)} = exact ${formatScore(hit.ranking_signals.exact)} + bm25 ${formatScore(hit.ranking_signals.bm25)} + vector ${formatScore(hit.ranking_signals.vector)}`}
                      aria-label="Ranking score details"
                    >
                      {formatScore(hit.ranking_signals.total)}
                    </span>
                    <button className="sv-hit-open" onClick={(e) => { e.stopPropagation(); openEditor(hit.note_id); }}>
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
          </div>
        )}

        {/* Cross-book results */}
        {crossBookMode && crossBookResults !== null && crossBookResults.length === 0 && query.trim() && !loading && (
          <div className="sv-state">No matches in other books.</div>
        )}
        {crossBookMode && crossBookResults && crossBookResults.length > 0 && (
          <div className="sv-results">
            {crossBookResults.map((hit) => (
              <div key={`${hit.book_path}/${hit.note_id}`} className="sv-hit sv-hit-cross-book">
                <div className="sv-hit-header">
                  <span className="sv-hit-title">{hit.title || '(untitled)'}</span>
                  <span className="sv-cross-book-badge">{hit.book_name}</span>
                </div>
                {hit.summary && <p className="sv-hit-summary">{hit.summary}</p>}
                <div className="sv-cross-book-link-hint">
                  Link syntax: <code>[{hit.title || hit.note_id}](book:{hit.book_name}/{hit.note_id})</code>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {!crossBookMode && preview && <RelatedCarousel noteId={preview} />}
    </div>
  );
}
