# Search

Search is a first-class feature. It must be fast, accurate, and deeply integrated with navigation.

## Search Methods

| Method | Description |
|---|---|
| **Exact match** | Literal string search |
| **Sparse retrieval (BM25)** | Keyword-based relevance ranking |
| **Vector search** | Semantic similarity using local embeddings |
| **Reciprocal Rank Fusion (RRF)** | Combines exact, BM25, and vector rankings into a single ranked list |

Document vectors are read from synced per-note embedding sidecars and reused by search, related notes,
duplicates, blind spots, category centroids, and clustering. Search embeds only the query at request time.
If the local model is unavailable, exact and BM25 rankings continue without the vector leg.

## Filtering

Search results can be filtered by category, similar to faceted search in e-commerce. This allows narrowing a broad query to a specific domain of the book.

## Search-Centered Graph View

After entering a query, the search view shows results as a **web of related content** centered on the search term. From this view, users can:
- Click into a note to read or edit it
- Open a note in book view or graph view with that note highlighted
- Start a new LLM chat using selected notes as context
- Add a new note as a neighbor of a selected note (a common workflow: search for a phrase near where you want to insert content, then add adjacent to it)

## Text Fade (Focus Mode)

In book view or long documents, an option to **fade less-relevant text** (make it more gray) helps users focus on the content most related to an active search term. More relevant passages remain at full opacity.

## Navigation from Search

When a note is selected from search results, the user can:
- Open it in graph view to see its connections
- Open it in book view to see its context in the narrative
- Modify it directly
- Insert a new note as its neighbor
