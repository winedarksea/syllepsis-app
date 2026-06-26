export interface WholeNoteFenceDetection {
  innerMarkdown: string;
  fenceLength: number;
  language: string;
}

const GENERIC_LANGUAGES = new Set(['', 'text', 'txt', 'plain', 'markdown', 'md']);

export function detectAccidentalWholeNoteCodeFence(body: string): WholeNoteFenceDetection | null {
  const normalized = body.replace(/\r\n/g, '\n').replace(/\r/g, '\n');
  const lines = normalized.split('\n');
  let first = 0;
  while (first < lines.length && lines[first].trim() === '') first += 1;
  let last = lines.length - 1;
  while (last >= first && lines[last].trim() === '') last -= 1;
  if (first >= last) return null;

  const opener = parseOpeningFence(lines[first]);
  if (!opener || !GENERIC_LANGUAGES.has(opener.language.toLowerCase())) return null;
  if (!isClosingFence(lines[last], opener.ch, opener.length)) return null;

  const innerMarkdown = lines.slice(first + 1, last).join('\n');
  if (!looksLikeProseMarkdown(innerMarkdown)) return null;

  return {
    innerMarkdown,
    fenceLength: opener.length,
    language: opener.language,
  };
}

function parseOpeningFence(line: string): { ch: '`' | '~'; length: number; language: string } | null {
  const trimmed = line.trim();
  const match = trimmed.match(/^(`{3,}|~{3,})(.*)$/);
  if (!match) return null;
  const fence = match[1];
  const rest = match[2].trim();
  const ch = fence[0] as '`' | '~';
  if (ch === '`' && rest.includes('`')) return null;
  return {
    ch,
    length: fence.length,
    language: rest.split(/\s+/)[0] ?? '',
  };
}

function isClosingFence(line: string, ch: '`' | '~', openerLength: number): boolean {
  const trimmed = line.trim();
  const match = trimmed.match(ch === '`' ? /^(`+)(.*)$/ : /^(~+)(.*)$/);
  return !!match && match[1].length >= openerLength && match[2].trim() === '';
}

function looksLikeProseMarkdown(innerMarkdown: string): boolean {
  const lines = innerMarkdown.split('\n');
  const hasHeading = lines.some((line) => /^#{1,6}\s+\S/.test(line.trim()));
  const hasList = lines.some((line) => /^\s*(?:[-*+]|\d+[.)])\s+\S/.test(line));
  const hasNestedFence = lines.some((line) => /^\s*(?:`{3,}|~{3,})/.test(line));
  const hasParagraphBreak = /\S[^\n]*\n\s*\n[^\n]*\S/.test(innerMarkdown);
  return hasHeading || hasList || hasNestedFence || hasParagraphBreak;
}
