// HTML sanitization for plugin-rendered output. Plugins (and especially third-party ones) are not
// trusted to produce safe markup, so every string they hand back is run through DOMPurify before it
// touches the DOM. Scripts, event handlers, and javascript: URLs are stripped.

import DOMPurify, { type Config } from 'dompurify';

// A deliberately broad-but-safe allowlist: enough for rendered code (spans/pre/code) and simple
// diagrams (inline SVG), while DOMPurify removes scripts, event handlers, and dangerous URLs.
const CONFIG: Config = {
  USE_PROFILES: { html: true, svg: true },
  ADD_TAGS: ['use'],
  FORBID_TAGS: ['script', 'style', 'iframe', 'object', 'embed', 'form'],
  FORBID_ATTR: ['style'],
};

/** Sanitize plugin-produced HTML (code blocks, SVG, etc.) into a string safe to inject. */
export function sanitizeHtml(dirty: string): string {
  return DOMPurify.sanitize(dirty, CONFIG);
}
