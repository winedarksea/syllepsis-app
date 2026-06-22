// Global app state via Zustand.

import { create } from 'zustand';
import type { BookInfo, Category } from '../types';

export type View =
  | 'book' | 'unsorted' | 'category' | 'editor' | 'search' | 'graph'
  | 'diagnostics' | 'worlds' | 'privacy' | 'packs';

interface AppStore {
  // Book
  book: BookInfo | null;
  setBook: (b: BookInfo | null) => void;

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

  // Theme
  theme: 'light' | 'dark';
  toggleTheme: () => void;
}

export const useStore = create<AppStore>((set) => ({
  book: null,
  setBook: (book) => set({ book }),

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

  theme: (window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light') as 'light' | 'dark',
  toggleTheme: () =>
    set((s) => ({ theme: s.theme === 'light' ? 'dark' : 'light' })),
}));
