import { describe, expect, it } from 'vitest';
import { clampFindIndex, findLiteralMatches } from './find';

describe('findLiteralMatches', () => {
  it('finds case-insensitive literal matches', () => {
    expect(findLiteralMatches('Alpha beta ALPHA', 'alpha')).toEqual([
      { start: 0, end: 5 },
      { start: 11, end: 16 },
    ]);
  });

  it('treats regex metacharacters as ordinary characters', () => {
    expect(findLiteralMatches('a.b a*b a[b a?b', 'a.b')).toEqual([{ start: 0, end: 3 }]);
    expect(findLiteralMatches('a.b a*b a[b a?b', 'a*b')).toEqual([{ start: 4, end: 7 }]);
    expect(findLiteralMatches('a.b a*b a[b a?b', 'a[b')).toEqual([{ start: 8, end: 11 }]);
    expect(findLiteralMatches('a.b a*b a[b a?b', 'a?b')).toEqual([{ start: 12, end: 15 }]);
  });

  it('uses non-overlapping matches', () => {
    expect(findLiteralMatches('aaaa', 'aa')).toEqual([
      { start: 0, end: 2 },
      { start: 2, end: 4 },
    ]);
  });

  it('returns no matches for empty or whitespace-only queries', () => {
    expect(findLiteralMatches('alpha', '')).toEqual([]);
    expect(findLiteralMatches('alpha', '   ')).toEqual([]);
  });
});

describe('clampFindIndex', () => {
  it('keeps the active match inside the available range', () => {
    expect(clampFindIndex(5, 3)).toBe(2);
    expect(clampFindIndex(-1, 3)).toBe(0);
    expect(clampFindIndex(1, 0)).toBe(0);
  });
});
