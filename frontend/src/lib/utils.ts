const PLACEHOLDER_TITLES = new Set(['New Note', 'New Title']);

// `snake_case` -> `Title Case`, e.g. for classification/status enum values shown in list rows.
// Memoized: the same small set of enum values gets re-humanized on every render of every row in
// a list, so caching by input avoids repeating the same two regex passes over and over.
const humanizeCache = new Map<string, string>();
export function humanize(value: string): string {
  const cached = humanizeCache.get(value);
  if (cached !== undefined) return cached;
  const result = value.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
  humanizeCache.set(value, result);
  return result;
}

export function displayTitle(title: string, summary?: string, body?: string): string {
  if (title && !PLACEHOLDER_TITLES.has(title)) return title;
  if (summary) {
    const line = summary.split('\n')[0].trim();
    if (line) return line.length > 80 ? line.slice(0, 80) + '…' : line;
  }
  if (body) {
    const line = body.split('\n').find((l) => l.trim().length > 0) ?? '';
    const stripped = line.replace(/^[#>\-*+\s]+/, '').trim();
    if (stripped) return stripped.length > 80 ? stripped.slice(0, 80) + '…' : stripped;
  }
  return '(untitled)';
}
