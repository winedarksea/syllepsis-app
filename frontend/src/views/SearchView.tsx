// The search-centred web of results: a query box, category facet filters, and ranked hits
// fused from exact + BM25 + vector retrieval (RRF). Selecting a hit previews its related
// notes and opens it in the editor.

import { useCallback, useEffect, useRef, useState } from 'react';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import type { SearchResults } from '../types';
import { RelatedCarousel } from '../components/RelatedCarousel';
import './SearchView.css';

export function SearchView() {
  const { openEditor } = useStore();
  const [query, setQuery] = useState('');
  const [activeFacets, setActiveFacets] = useState<string[]>([]);
  const [results, setResults] = useState<SearchResults | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const debounce = useRef<ReturnType<typeof setTimeout> | null>(null);

  const run = useCallback((q: string, facets: string[]) => {
    if (!q.trim()) { setResults(null); return; }
    setLoading(true);
    api.search(q, facets)
      .then((r) => { setResults(r); setError(null); })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, []);

  // Debounced search-as-you-type.
  useEffect(() => {
    if (debounce.current) clearTimeout(debounce.current);
    debounce.current = setTimeout(() => run(query, activeFacets), 180);
    return () => { if (debounce.current) clearTimeout(debounce.current); };
  }, [query, activeFacets, run]);

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
          placeholder="Search your book — exact, keyword, and meaning…"
          autoFocus
        />
      </div>

      {results && results.facets.length > 0 && (
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
          <div className="sv-state sv-hint">Type to search across every note.</div>
        )}
        {loading && <div className="sv-state">Searching…</div>}
        {results && results.hits.length === 0 && query.trim() && !loading && (
          <div className="sv-state">No matches.</div>
        )}

        {results && results.hits.length > 0 && (
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
                  <button className="sv-hit-open" onClick={(e) => { e.stopPropagation(); openEditor(hit.note_id); }}>
                    Open
                  </button>
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
      </div>

      {preview && <RelatedCarousel noteId={preview} />}
    </div>
  );
}
