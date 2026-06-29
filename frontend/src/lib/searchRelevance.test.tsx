import { describe, expect, it } from 'vitest';
import type { SearchHit } from '../types';
import { formatSearchRelevancePercent, searchRelevance } from './searchRelevance';

function hit(signals: SearchHit['ranking_signals']): SearchHit {
  return {
    note_id: 'note-1',
    title: 'Note',
    summary: '',
    snippet: '',
    categories: [],
    score: signals.total,
    ranking_signals: signals,
    object_type: 'note',
    updated: '2026-06-28T00:00:00Z',
    starred: false,
    body_len: 100,
    archived: false,
  };
}

describe('searchRelevance', () => {
  it('uses semantic similarity as an absolute relevance signal', () => {
    expect(formatSearchRelevancePercent(hit({
      exact: 0,
      bm25: 0,
      vector: 1 / 61,
      total: 1 / 61,
      vector_similarity: 0.27,
    }))).toBe('27%');
  });

  it('does not turn the best rank-only hit into 100%', () => {
    const relevance = searchRelevance(hit({
      exact: 1 / 61,
      bm25: 1 / 61,
      vector: 0,
      total: 2 / 61,
      vector_similarity: 0,
    }));

    expect(relevance).toBeGreaterThan(0.7);
    expect(relevance).toBeLessThan(0.75);
  });

  it('adds a small boost when independent retrievers agree', () => {
    expect(formatSearchRelevancePercent(hit({
      exact: 1 / 61,
      bm25: 1 / 62,
      vector: 1 / 63,
      total: 1 / 61 + 1 / 62 + 1 / 63,
      vector_similarity: 0.62,
    }))).toBe('74%');
  });
});
