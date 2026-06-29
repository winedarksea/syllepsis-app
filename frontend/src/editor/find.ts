export interface EditorFindMatch {
  start: number;
  end: number;
}

export function normalizeFindPattern(pattern: string): string {
  return pattern.trim();
}

export function findLiteralMatches(text: string, pattern: string): EditorFindMatch[] {
  const normalizedPattern = normalizeFindPattern(pattern);
  if (!normalizedPattern) return [];

  const lowerText = text.toLowerCase();
  const lowerPattern = normalizedPattern.toLowerCase();
  const matches: EditorFindMatch[] = [];
  let searchFrom = 0;

  while (searchFrom <= lowerText.length) {
    const start = lowerText.indexOf(lowerPattern, searchFrom);
    if (start === -1) break;
    const end = start + normalizedPattern.length;
    matches.push({ start, end });
    searchFrom = end;
  }

  return matches;
}

export function clampFindIndex(index: number, matchCount: number): number {
  if (matchCount <= 0) return 0;
  if (index < 0) return 0;
  if (index >= matchCount) return matchCount - 1;
  return index;
}
