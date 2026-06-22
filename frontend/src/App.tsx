import { useCallback, useEffect } from 'react';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useStore } from './lib/store';
import { api } from './lib/api';
import { Sidebar } from './components/Sidebar';
import { BookView } from './views/BookView';
import { UnsortedQueue } from './views/UnsortedQueue';
import { CategoryView } from './views/CategoryView';
import { SearchView } from './views/SearchView';
import { GraphView } from './views/GraphView';
import { WorldView } from './views/WorldView';
import { PrivacyView } from './views/PrivacyView';
import { PacksView } from './views/PacksView';
import { Diagnostics } from './views/Diagnostics';
import { Editor } from './editor/Editor';
import './App.css';

// ──────────────────────────────────────────────
// First-launch screen: open or create a book
// ──────────────────────────────────────────────
function BookPicker() {
  const { setBook, setCategories } = useStore();

  const handleOpen = useCallback(async () => {
    const selected = await openDialog({ directory: true, multiple: false, title: 'Open Syllepsis Book' });
    if (!selected || typeof selected !== 'string') return;
    const info = await api.openBook(selected);
    setBook(info);
    const cats = await api.allCategories();
    setCategories(cats);
  }, [setBook, setCategories]);

  const handleCreate = useCallback(async () => {
    const dir = await openDialog({ directory: true, multiple: false, title: 'Choose folder for new book' });
    if (!dir || typeof dir !== 'string') return;
    const name = prompt('Book name:');
    if (!name?.trim()) return;
    const info = await api.createBook(dir, name.trim());
    setBook(info);
    setCategories([]);
  }, [setBook, setCategories]);

  return (
    <div className="picker-root">
      <div className="picker-card">
        <div className="picker-logo">✦</div>
        <h1 className="picker-title">Syllepsis</h1>
        <p className="picker-subtitle">Your local-first knowledge book</p>
        <div className="picker-actions">
          <button className="picker-btn picker-btn-primary" onClick={handleOpen}>
            Open Book
          </button>
          <button className="picker-btn picker-btn-secondary" onClick={handleCreate}>
            Create New Book
          </button>
        </div>
      </div>
    </div>
  );
}

// ──────────────────────────────────────────────
// Main workspace (book is open)
// ──────────────────────────────────────────────
function Workspace() {
  const { view, editingNoteId, setCategories, setUnsortedCount, openEditor } = useStore();

  // Refresh sidebar data on view change (i.e. when returning from the editor).
  useEffect(() => {
    api.allCategories().then(setCategories).catch(console.error);
    api.unsortedNotes().then((ns) => setUnsortedCount(ns.length)).catch(console.error);
  }, [view, setCategories, setUnsortedCount]);

  const handleNewNote = useCallback(async () => {
    const note = await api.createNote('note', 'New Note');
    openEditor(note.id);
  }, [openEditor]);

  return (
    <div className="workspace">
      <Sidebar onNewNote={handleNewNote} />
      <main className="workspace-main">
        {view === 'editor' && editingNoteId ? (
          <Editor noteId={editingNoteId} />
        ) : view === 'book' ? (
          <BookView />
        ) : view === 'category' ? (
          <CategoryView />
        ) : view === 'search' ? (
          <SearchView />
        ) : view === 'graph' ? (
          <GraphView />
        ) : view === 'worlds' ? (
          <WorldView />
        ) : view === 'privacy' ? (
          <PrivacyView />
        ) : view === 'packs' ? (
          <PacksView />
        ) : view === 'diagnostics' ? (
          <Diagnostics />
        ) : (
          <UnsortedQueue />
        )}
      </main>
    </div>
  );
}

// ──────────────────────────────────────────────
// Root
// ──────────────────────────────────────────────
export default function App() {
  const { book, theme } = useStore();

  return (
    <div data-theme={theme} style={{ height: '100%' }}>
      {book ? <Workspace /> : <BookPicker />}
    </div>
  );
}
