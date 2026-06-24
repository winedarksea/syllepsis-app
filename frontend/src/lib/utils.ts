const PLACEHOLDER_TITLES = new Set(['New Note', 'New Title']);

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
