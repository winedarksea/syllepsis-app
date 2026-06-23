// Theme families. Each theme defines a full set of color tokens for light and dark; the
// Light/Dark/System control then picks the mode within the active theme.
//
// Built-in themes ship in code; custom themes are imported from JSON files and stored in
// localStorage (see store.ts). The active theme's resolved-mode token map is applied as inline
// CSS custom properties on the app root (App.tsx), overriding the base light.css/dark.css.

export type ThemeVars = Record<string, string>;

// Bounded visual-personality enum variants — applied as data-attributes on the app root,
// driving CSS selectors in GraphView.css, BookView.css, and WorldView.css.
export interface ThemeStyle {
  graphEdge?: 'weave' | 'glow' | 'plain';
  graphNode?: 'disc' | 'star' | 'hex';
  divider?:   'rule' | 'knot' | 'dotdash';
  grid?:      'survey' | 'dots' | 'contour' | 'none';
  iconSet?:   string;
}

// Safe per-slot SVG override: only path-data + viewBox, rendered as controlled <svg><path>.
// No raw markup ever touches the DOM — only the listed fields reach the renderer.
export type ThemeIcon = { viewBox?: string; path: string | string[] };

// The named nav/signature slots that themes may override with custom glyphs.
export type SignatureSlot = 'book' | 'unsorted' | 'search' | 'graph' | 'worlds' | 'packs' | 'new' | 'sync';

export const SIGNATURE_SLOTS: readonly SignatureSlot[] = [
  'book', 'unsorted', 'search', 'graph', 'worlds', 'packs', 'new', 'sync',
];

// Material Symbols fallback ligature name for each slot (used when no icon set / override).
export const SLOT_FALLBACK: Record<SignatureSlot, string> = {
  book: 'menu_book',
  unsorted: 'inbox',
  search: 'search',
  graph: 'hub',
  worlds: 'map',
  packs: 'inventory_2',
  new: 'add',
  sync: 'cloud_off',
};

export type ThemeIcons = Partial<Record<SignatureSlot, ThemeIcon>>;

export interface Theme {
  id: string;
  name: string;
  author?: string;
  builtin?: boolean;
  light: ThemeVars;
  dark: ThemeVars;
  style?: ThemeStyle;
  icons?: ThemeIcons;
}

// Every color/shadow token a complete theme defines. Imported themes are merged over the Nordic
// base, so a partial theme still renders (unspecified tokens fall back to Nordic).
export const TOKEN_KEYS: readonly string[] = [
  '--color-bg', '--color-surface', '--color-surface-raised', '--color-border', '--color-border-focus',
  '--color-text', '--color-text-secondary', '--color-text-tertiary', '--color-text-inverse',
  '--color-accent', '--color-accent-hover', '--color-accent-subtle',
  '--color-secondary', '--color-secondary-subtle', '--color-tertiary', '--color-tertiary-subtle',
  '--color-error', '--color-heading', '--color-link',
  '--color-tag-bg', '--color-tag-text', '--color-tag-border',
  '--color-cloze-bg', '--color-cloze-border', '--color-cloze-text',
  '--color-todo-pending', '--color-todo-done', '--color-unsorted-badge',
  '--color-sidebar-bg', '--color-sidebar-active', '--color-sidebar-text', '--color-scaffold',
  '--shadow-sm', '--shadow-md',
];

// ── Nordic / Icelandic (default) — mirrors theme/light.css and theme/dark.css ──
const NORDIC: Theme = {
  id: 'nordic',
  name: 'Nordic / Icelandic',
  builtin: true,
  style: { graphEdge: 'weave', graphNode: 'hex', divider: 'knot', grid: 'survey', iconSet: 'nordic' },
  light: {
    '--color-bg': '#e8e5de', '--color-surface': '#f3f0e8', '--color-surface-raised': '#dde4e6',
    '--color-border': '#9a9992', '--color-border-focus': '#2f7fa3',
    '--color-text': '#1f2528', '--color-text-secondary': '#555e61', '--color-text-tertiary': '#8a8e89',
    '--color-text-inverse': '#f3f0e8',
    '--color-accent': '#2f7fa3', '--color-accent-hover': '#266a8a', '--color-accent-subtle': '#d7e3e8',
    '--color-secondary': '#6f8767', '--color-secondary-subtle': '#dde3d7',
    '--color-tertiary': '#a45a36', '--color-tertiary-subtle': '#f0e1d8',
    '--color-error': '#b0433e', '--color-heading': '#18242a', '--color-link': '#2f7fa3',
    '--color-tag-bg': '#d7e3e8', '--color-tag-text': '#266a8a', '--color-tag-border': '#b6cdd6',
    '--color-cloze-bg': '#efe3d6', '--color-cloze-border': '#c9a87e', '--color-cloze-text': '#6e3f22',
    '--color-todo-pending': '#a45a36', '--color-todo-done': '#6f8767', '--color-unsorted-badge': '#a45a36',
    '--color-sidebar-bg': '#f3f0e8', '--color-sidebar-active': '#dde4e6', '--color-sidebar-text': '#555e61',
    '--color-scaffold': '#9a9992', '--shadow-sm': 'none', '--shadow-md': 'none',
  },
  dark: {
    '--color-bg': '#111416', '--color-surface': '#20262a', '--color-surface-raised': '#2c3438',
    '--color-border': '#3a4145', '--color-border-focus': '#6fa8c7',
    '--color-text': '#ece8de', '--color-text-secondary': '#b8c0be', '--color-text-tertiary': '#7e8785',
    '--color-text-inverse': '#111416',
    '--color-accent': '#6fa8c7', '--color-accent-hover': '#84b8d4', '--color-accent-subtle': '#233038',
    '--color-secondary': '#5e7e58', '--color-secondary-subtle': '#24302a',
    '--color-tertiary': '#c8754d', '--color-tertiary-subtle': '#2e2117',
    '--color-error': '#e0736a', '--color-heading': '#f2eee4', '--color-link': '#6fa8c7',
    '--color-tag-bg': '#233038', '--color-tag-text': '#84b8d4', '--color-tag-border': '#355063',
    '--color-cloze-bg': '#2e2a14', '--color-cloze-border': '#807040', '--color-cloze-text': '#d4c060',
    '--color-todo-pending': '#c8754d', '--color-todo-done': '#7aaa68', '--color-unsorted-badge': '#c8754d',
    '--color-sidebar-bg': '#1a1f22', '--color-sidebar-active': '#2c3438', '--color-sidebar-text': '#b8c0be',
    '--color-scaffold': '#6e7779', '--shadow-sm': 'none', '--shadow-md': 'none',
  },
};

// ── Navigator's Archive — abstract medieval / star chart (theme-style.md) ──
const NAVIGATORS_ARCHIVE: Theme = {
  id: 'navigators-archive',
  name: "Navigator's Archive",
  builtin: true,
  style: { graphEdge: 'glow', graphNode: 'star', divider: 'dotdash', grid: 'contour', iconSet: 'archive' },
  // Light "The Logbook": warm parchment surfaces, brass/gold accents, ink/indigo secondary.
  light: {
    '--color-bg': '#f1e7d0', '--color-surface': '#f7efdd', '--color-surface-raised': '#e9dcc0',
    '--color-border': '#b8a888', '--color-border-focus': '#a87f3c',
    '--color-text': '#2b2418', '--color-text-secondary': '#5c5240', '--color-text-tertiary': '#8a7d63',
    '--color-text-inverse': '#f7efdd',
    '--color-accent': '#a87f3c', '--color-accent-hover': '#8c6a30', '--color-accent-subtle': '#ece0c6',
    '--color-secondary': '#3f4a6b', '--color-secondary-subtle': '#dcdfe9',
    '--color-tertiary': '#8a5a2b', '--color-tertiary-subtle': '#ecdcc6',
    '--color-error': '#a23b32', '--color-heading': '#241d12', '--color-link': '#8c6a30',
    '--color-tag-bg': '#e7ddc4', '--color-tag-text': '#8c6a30', '--color-tag-border': '#cbb98f',
    '--color-cloze-bg': '#ece0c6', '--color-cloze-border': '#c9a87e', '--color-cloze-text': '#6e3f22',
    '--color-todo-pending': '#a87f3c', '--color-todo-done': '#5e7e58', '--color-unsorted-badge': '#a23b32',
    '--color-sidebar-bg': '#ece0c6', '--color-sidebar-active': '#e0d0ad', '--color-sidebar-text': '#5c5240',
    '--color-scaffold': '#b8a888', '--shadow-sm': 'none', '--shadow-md': 'none',
  },
  // Dark "The Constellation": deep-space indigo/slate ground, glowing gold/brass accents.
  dark: {
    '--color-bg': '#0f1320', '--color-surface': '#181d2e', '--color-surface-raised': '#222840',
    '--color-border': '#2f3650', '--color-border-focus': '#c9a44a',
    '--color-text': '#e6e3d6', '--color-text-secondary': '#b3b0c2', '--color-text-tertiary': '#7c7a90',
    '--color-text-inverse': '#0f1320',
    '--color-accent': '#c9a44a', '--color-accent-hover': '#ddba63', '--color-accent-subtle': '#2a2740',
    '--color-secondary': '#6b7bbd', '--color-secondary-subtle': '#1f2540',
    '--color-tertiary': '#b9763f', '--color-tertiary-subtle': '#2c2018',
    '--color-error': '#e0736a', '--color-heading': '#f0ecdd', '--color-link': '#c9a44a',
    '--color-tag-bg': '#232846', '--color-tag-text': '#c9a44a', '--color-tag-border': '#3a4168',
    '--color-cloze-bg': '#2a2614', '--color-cloze-border': '#807040', '--color-cloze-text': '#d4c060',
    '--color-todo-pending': '#b9763f', '--color-todo-done': '#7aaa68', '--color-unsorted-badge': '#b9763f',
    '--color-sidebar-bg': '#0c1018', '--color-sidebar-active': '#222840', '--color-sidebar-text': '#b3b0c2',
    '--color-scaffold': '#4a5170', '--shadow-sm': 'none', '--shadow-md': 'none',
  },
};

export const BUILTIN_THEMES: Theme[] = [NORDIC, NAVIGATORS_ARCHIVE];

export const DEFAULT_THEME_ID = NORDIC.id;

export function allThemes(custom: Theme[]): Theme[] {
  return [...BUILTIN_THEMES, ...custom];
}

export function themeById(id: string, custom: Theme[]): Theme | undefined {
  return allThemes(custom).find((t) => t.id === id);
}

/** The token map to apply for `id` in `mode`, falling back to the Nordic default. */
export function resolveThemeVars(id: string, mode: 'light' | 'dark', custom: Theme[]): ThemeVars {
  const theme = themeById(id, custom) ?? NORDIC;
  return theme[mode];
}

/** A few representative swatches for a theme card preview, in the given mode. */
export function themeSwatches(theme: Theme, mode: 'light' | 'dark'): string[] {
  const v = theme[mode];
  return [v['--color-bg'], v['--color-surface'], v['--color-accent'], v['--color-secondary']];
}

/** Resolved ThemeStyle with all defaults filled in. Mode-independent. */
export function resolveThemeStyle(id: string, custom: Theme[]): Required<ThemeStyle> {
  const theme = themeById(id, custom) ?? NORDIC;
  return {
    graphEdge: theme.style?.graphEdge ?? 'weave',
    graphNode: theme.style?.graphNode ?? 'disc',
    divider:   theme.style?.divider   ?? 'dotdash',
    grid:      theme.style?.grid      ?? 'survey',
    iconSet:   theme.style?.iconSet   ?? 'material',
  };
}

/** Merged icon overrides: theme.icons wins over the active iconSet's entries. Mode-independent. */
export function resolveThemeIcons(id: string, custom: Theme[]): ThemeIcons {
  const theme = themeById(id, custom) ?? NORDIC;
  return theme.icons ?? {};
}

/** Serialize a theme to a JSON string usable as an import template. */
export function themeToJson(theme: Theme): string {
  return JSON.stringify(
    {
      id: theme.id,
      name: theme.name,
      author: theme.author ?? '',
      style: theme.style ?? {},
      icons: theme.icons ?? {},
      light: theme.light,
      dark: theme.dark,
    },
    null,
    2,
  );
}

function slugify(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '') || 'custom-theme';
}

function pickTokens(raw: unknown): ThemeVars {
  const out: ThemeVars = {};
  if (raw && typeof raw === 'object') {
    for (const [key, value] of Object.entries(raw as Record<string, unknown>)) {
      if (TOKEN_KEYS.includes(key) && typeof value === 'string' && value.trim()) {
        out[key] = value.trim();
      }
    }
  }
  return out;
}

const VALID_GRAPH_EDGE = new Set(['weave', 'glow', 'plain']);
const VALID_GRAPH_NODE = new Set(['disc', 'star', 'hex']);
const VALID_DIVIDER    = new Set(['rule', 'knot', 'dotdash']);
const VALID_GRID       = new Set(['survey', 'dots', 'contour', 'none']);

function pickStyle(raw: unknown): ThemeStyle | undefined {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return undefined;
  const s = raw as Record<string, unknown>;
  const out: ThemeStyle = {};
  if (typeof s.graphEdge === 'string' && VALID_GRAPH_EDGE.has(s.graphEdge)) out.graphEdge = s.graphEdge as ThemeStyle['graphEdge'];
  if (typeof s.graphNode === 'string' && VALID_GRAPH_NODE.has(s.graphNode)) out.graphNode = s.graphNode as ThemeStyle['graphNode'];
  if (typeof s.divider   === 'string' && VALID_DIVIDER.has(s.divider))     out.divider   = s.divider   as ThemeStyle['divider'];
  if (typeof s.grid      === 'string' && VALID_GRID.has(s.grid))           out.grid      = s.grid      as ThemeStyle['grid'];
  if (typeof s.iconSet   === 'string' && s.iconSet.trim())                  out.iconSet   = s.iconSet.trim();
  return Object.keys(out).length ? out : undefined;
}

function isThemeIcon(v: unknown): v is ThemeIcon {
  if (!v || typeof v !== 'object' || Array.isArray(v)) return false;
  const obj = v as Record<string, unknown>;
  const path = obj.path;
  if (typeof path === 'string' && path.trim()) return true;
  if (Array.isArray(path) && path.every((p) => typeof p === 'string')) return true;
  return false;
}

function pickIcons(raw: unknown): ThemeIcons | undefined {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return undefined;
  const src = raw as Record<string, unknown>;
  const out: ThemeIcons = {};
  for (const slot of SIGNATURE_SLOTS) {
    const v = src[slot];
    if (isThemeIcon(v)) out[slot] = v as ThemeIcon;
  }
  return Object.keys(out).length ? out : undefined;
}

/**
 * Validate and normalize an imported theme. Provided tokens are merged over the Nordic base so the
 * result is always a complete theme. Returns either a normalized theme or a human-readable error.
 */
export function normalizeImportedTheme(text: string): { theme?: Theme; error?: string } {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return { error: 'Not valid JSON.' };
  }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    return { error: 'Theme must be a JSON object.' };
  }
  const obj = parsed as Record<string, unknown>;
  const name = typeof obj.name === 'string' ? obj.name.trim() : '';
  if (!name) return { error: 'Theme is missing a "name".' };

  const light = pickTokens(obj.light);
  const dark = pickTokens(obj.dark);
  if (Object.keys(light).length === 0 && Object.keys(dark).length === 0) {
    return { error: 'Theme defines no recognized color tokens under "light"/"dark".' };
  }

  const id = typeof obj.id === 'string' && obj.id.trim() ? slugify(obj.id) : slugify(name);
  const author = typeof obj.author === 'string' ? obj.author.trim() : undefined;
  const style = pickStyle(obj.style);
  const icons = pickIcons(obj.icons);
  return {
    theme: {
      id,
      name,
      author: author || undefined,
      light: { ...NORDIC.light, ...light },
      dark: { ...NORDIC.dark, ...dark },
      ...(style ? { style } : {}),
      ...(icons ? { icons } : {}),
    },
  };
}
