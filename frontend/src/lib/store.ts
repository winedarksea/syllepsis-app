// Global app state via Zustand.

import { create } from 'zustand';
import type { BookInfo, Category, GraphMode } from '../types';
import type { Theme } from '../theme/themes';
import { DEFAULT_THEME_ID, themeById } from '../theme/themes';

export type View =
  | 'book' | 'unsorted' | 'category' | 'editor' | 'search' | 'graph'
  | 'diagnostics' | 'worlds' | 'privacy' | 'packs' | 'text_import' | 'stats' | 'style_cards'
  | 'settings';

export type ThemePref = 'light' | 'dark' | 'system';

const THEME_PREF_KEY = 'syllepsis.themePref';
const THEME_ID_KEY = 'syllepsis.themeId';
const CUSTOM_THEMES_KEY = 'syllepsis.customThemes';
const HIDE_UNSORTED_BADGE_KEY = 'syllepsis.hideUnsortedBadge';

function readSystemTheme(): 'light' | 'dark' {
  return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function readThemePref(): ThemePref {
  const stored = localStorage.getItem(THEME_PREF_KEY);
  return stored === 'light' || stored === 'dark' || stored === 'system' ? stored : 'system';
}

function resolveTheme(pref: ThemePref): 'light' | 'dark' {
  return pref === 'system' ? readSystemTheme() : pref;
}

function readCustomThemes(): Theme[] {
  try {
    const raw = localStorage.getItem(CUSTOM_THEMES_KEY);
    const parsed = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed) ? (parsed as Theme[]) : [];
  } catch {
    return [];
  }
}

function readThemeId(custom: Theme[]): string {
  const stored = localStorage.getItem(THEME_ID_KEY);
  // Fall back to the default if the stored family was deleted or never existed.
  return stored && themeById(stored, custom) ? stored : DEFAULT_THEME_ID;
}

interface AppStore {
  // Book
  book: BookInfo | null;
  setBook: (b: BookInfo | null) => void;
  closeBook: () => void;

  // Active view
  view: View;
  setView: (v: View) => void;

  // Category selected in sidebar
  activeCategory: string | null;
  setActiveCategory: (name: string | null) => void;

  // World selected for the overlay view
  activeWorld: string | null;
  setActiveWorld: (id: string | null) => void;

  // Note open in the editor
  editingNoteId: string | null;
  openEditor: (id: string) => void;
  closeEditor: () => void;

  // Cached category list (refreshed when categories change)
  categories: Category[];
  setCategories: (cats: Category[]) => void;

  // Unsorted count badge
  unsortedCount: number;
  setUnsortedCount: (n: number) => void;
  hideUnsortedBadge: boolean;
  setHideUnsortedBadge: (v: boolean) => void;

  // Diagnostics issue count (persisted per-book by Diagnostics view; 0 = clean/unknown)
  diagnosticsIssueCount: number;
  setDiagnosticsIssueCount: (n: number) => void;

  // Graph display preferences (session-scoped; intentionally not persisted).
  showAllGraphTitles: boolean;
  setShowAllGraphTitles: (show: boolean) => void;
  graphMode: GraphMode;
  setGraphMode: (mode: GraphMode) => void;
  graphSimilarityThreshold: number;
  setGraphSimilarityThreshold: (threshold: number) => void;
  graphAdvancedOpen: boolean;
  setGraphAdvancedOpen: (open: boolean) => void;
  graphPillarsNeighbors: number;
  setGraphPillarsNeighbors: (neighbors: number) => void;
  graphCommunitiesNeighbors: number;
  setGraphCommunitiesNeighbors: (neighbors: number) => void;
  graphDensityNeighbors: number;
  setGraphDensityNeighbors: (neighbors: number) => void;
  graphKmeansK: number;
  setGraphKmeansK: (clusters: number) => void;
  graphLouvainResolution: number;
  setGraphLouvainResolution: (resolution: number) => void;
  graphHdbscanMinClusterSize: number;
  setGraphHdbscanMinClusterSize: (size: number) => void;

  // Fenced-code languages claimed by code-block-renderer plugins (lower-cased). Loaded once at
  // startup; the editor maps these languages to a rendered PluginBlockNode instead of plain code.
  pluginRenderLanguages: string[];
  setPluginRenderLanguages: (languages: string[]) => void;
  // True once list_plugins() has settled (success or error). Gates the Lexical editor mount so
  // InitBodyPlugin always fires with the final pluginRenderLanguages, avoiding a race condition.
  pluginsLoaded: boolean;
  setPluginsLoaded: (loaded: boolean) => void;

  // Theme: `themePref` is the light/dark/system choice (persisted); `theme` is the resolved mode
  // applied to the DOM. When the pref is 'system', `theme` tracks the OS color scheme.
  themePref: ThemePref;
  theme: 'light' | 'dark';
  setThemePref: (pref: ThemePref) => void;
  toggleTheme: () => void;
  // Re-resolve from the OS color scheme; a no-op unless the pref is 'system'.
  syncSystemTheme: () => void;

  // Theme family: `themeId` selects which palette (built-in or imported custom) is active; each
  // family carries its own light & dark token sets. `customThemes` are user-imported (persisted).
  themeId: string;
  customThemes: Theme[];
  setThemeId: (id: string) => void;
  addCustomTheme: (theme: Theme) => void;
  removeCustomTheme: (id: string) => void;
}

export const useStore = create<AppStore>((set) => ({
  book: null,
  setBook: (book) => {
    // Seed the diagnostics badge from the persisted count so it shows without visiting the view.
    let diagnosticsIssueCount = 0;
    if (book) {
      const stored = parseInt(localStorage.getItem(`syllepsis.diag.issueCount.${book.path}`) ?? '0', 10);
      if (!isNaN(stored)) diagnosticsIssueCount = stored;
    }
    set({ book, diagnosticsIssueCount });
  },
  // Return to the launch screen, clearing any per-book state so the next book opens clean.
  closeBook: () => set({
    book: null,
    view: 'unsorted',
    editingNoteId: null,
    activeCategory: null,
    activeWorld: null,
    categories: [],
    unsortedCount: 0,
    diagnosticsIssueCount: 0,
  }),

  view: 'unsorted',
  setView: (view) => set({ view }),

  activeCategory: null,
  setActiveCategory: (activeCategory) => set({ activeCategory }),

  activeWorld: null,
  setActiveWorld: (activeWorld) => set({ activeWorld }),

  editingNoteId: null,
  openEditor: (id) => set({ editingNoteId: id, view: 'editor' }),
  closeEditor: () => set({ editingNoteId: null, view: 'unsorted' }),

  categories: [],
  setCategories: (categories) => set({ categories }),

  unsortedCount: 0,
  setUnsortedCount: (unsortedCount) => set({ unsortedCount }),
  hideUnsortedBadge: localStorage.getItem(HIDE_UNSORTED_BADGE_KEY) === 'true',
  setHideUnsortedBadge: (hideUnsortedBadge) => {
    localStorage.setItem(HIDE_UNSORTED_BADGE_KEY, String(hideUnsortedBadge));
    set({ hideUnsortedBadge });
  },

  diagnosticsIssueCount: 0,
  setDiagnosticsIssueCount: (diagnosticsIssueCount) => set({ diagnosticsIssueCount }),

  showAllGraphTitles: false,
  setShowAllGraphTitles: (showAllGraphTitles) => set({ showAllGraphTitles }),
  graphMode: 'categories',
  setGraphMode: (graphMode) => set({ graphMode }),
  graphSimilarityThreshold: 0.35,
  setGraphSimilarityThreshold: (graphSimilarityThreshold) => set({ graphSimilarityThreshold }),
  graphAdvancedOpen: false,
  setGraphAdvancedOpen: (graphAdvancedOpen) => set({ graphAdvancedOpen }),
  graphPillarsNeighbors: 50,
  setGraphPillarsNeighbors: (graphPillarsNeighbors) => set({ graphPillarsNeighbors }),
  graphCommunitiesNeighbors: 8,
  setGraphCommunitiesNeighbors: (graphCommunitiesNeighbors) => set({ graphCommunitiesNeighbors }),
  graphDensityNeighbors: 15,
  setGraphDensityNeighbors: (graphDensityNeighbors) => set({ graphDensityNeighbors }),
  graphKmeansK: 5,
  setGraphKmeansK: (graphKmeansK) => set({ graphKmeansK }),
  graphLouvainResolution: 1,
  setGraphLouvainResolution: (graphLouvainResolution) => set({ graphLouvainResolution }),
  graphHdbscanMinClusterSize: 5,
  setGraphHdbscanMinClusterSize: (graphHdbscanMinClusterSize) =>
    set({ graphHdbscanMinClusterSize }),

  pluginRenderLanguages: [],
  setPluginRenderLanguages: (pluginRenderLanguages) => set({ pluginRenderLanguages }),
  pluginsLoaded: false,
  setPluginsLoaded: (pluginsLoaded) => set({ pluginsLoaded }),

  themePref: readThemePref(),
  theme: resolveTheme(readThemePref()),
  setThemePref: (themePref) => {
    localStorage.setItem(THEME_PREF_KEY, themePref);
    set({ themePref, theme: resolveTheme(themePref) });
  },
  toggleTheme: () =>
    set((s) => {
      const next = s.theme === 'light' ? 'dark' : 'light';
      localStorage.setItem(THEME_PREF_KEY, next);
      return { themePref: next, theme: next };
    }),
  syncSystemTheme: () =>
    set((s) => (s.themePref === 'system' ? { theme: readSystemTheme() } : {})),

  themeId: readThemeId(readCustomThemes()),
  customThemes: readCustomThemes(),
  setThemeId: (themeId) => {
    localStorage.setItem(THEME_ID_KEY, themeId);
    set({ themeId });
  },
  addCustomTheme: (theme) =>
    set((s) => {
      // Replace any existing theme with the same id, then select the imported one.
      const customThemes = [...s.customThemes.filter((t) => t.id !== theme.id), theme];
      localStorage.setItem(CUSTOM_THEMES_KEY, JSON.stringify(customThemes));
      localStorage.setItem(THEME_ID_KEY, theme.id);
      return { customThemes, themeId: theme.id };
    }),
  removeCustomTheme: (id) =>
    set((s) => {
      const customThemes = s.customThemes.filter((t) => t.id !== id);
      localStorage.setItem(CUSTOM_THEMES_KEY, JSON.stringify(customThemes));
      // If the deleted family was active, fall back to the default.
      const themeId = s.themeId === id ? DEFAULT_THEME_ID : s.themeId;
      localStorage.setItem(THEME_ID_KEY, themeId);
      return { customThemes, themeId };
    }),
}));
