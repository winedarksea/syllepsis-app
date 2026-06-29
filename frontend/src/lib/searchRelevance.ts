import type { SearchHit } from '../types';

const STRONG_RRF_CHANNEL_CONTRIBUTION = 1 / 61;
const STRONG_LEXICAL_SIGNAL = STRONG_RRF_CHANNEL_CONTRIBUTION * 2;
const MAX_LEXICAL_RELEVANCE = 0.72;
const TWO_CHANNEL_AGREEMENT_BOOST = 0.04;
const THREE_CHANNEL_AGREEMENT_BOOST = 0.08;

function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(1, value));
}

export function searchRelevance(hit: SearchHit): number {
  const signals = hit.ranking_signals;
  const semanticRelevance = clamp01(signals.vector_similarity);
  const lexicalSignal = clamp01((signals.exact + signals.bm25) / STRONG_LEXICAL_SIGNAL);
  const lexicalRelevance = lexicalSignal * MAX_LEXICAL_RELEVANCE;
  const channelCount = [signals.exact, signals.bm25, signals.vector].filter((signal) => signal > 0).length;
  const agreementBoost =
    channelCount >= 3
      ? THREE_CHANNEL_AGREEMENT_BOOST
      : channelCount >= 2
        ? TWO_CHANNEL_AGREEMENT_BOOST
        : 0;

  const primaryRelevance = Math.max(semanticRelevance, lexicalRelevance);
  return clamp01(primaryRelevance + (1 - primaryRelevance) * agreementBoost);
}

export function formatSearchRelevancePercent(hit: SearchHit): string {
  return `${Math.round(searchRelevance(hit) * 100)}%`;
}
