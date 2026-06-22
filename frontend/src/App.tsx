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
import type { BookInfo } from './types';
import './App.css';

interface BookCreationDetails {
  name: string;
  language?: string;
  location?: string;
}

function missingBookFilesMessage(info: BookInfo): string {
  const missingFiles = info.open_warning?.missing_reserved_files.join(', ') ?? '';
  return `This folder is missing ${missingFiles}, so it may not be a Syllepsis book.`;
}

function promptForBookCreationDetails(defaultName = ''): BookCreationDetails | null {
  const name = prompt('Book name:', defaultName);
  if (!name?.trim()) return null;

  const language = prompt('Primary language:', 'en');
  if (language === null) return null;

  const location = prompt('Location (optional):', '');
  if (location === null) return null;

  return {
    name: name.trim(),
    language: language.trim() || undefined,
    location: location.trim() || undefined,
  };
}

// ──────────────────────────────────────────────
// First-launch screen: open or create a book
// ──────────────────────────────────────────────
function BookPicker() {
  const { setBook, setCategories } = useStore();

  const finishOpeningBook = useCallback(async (info: BookInfo) => {
    setBook(info);
    const cats = await api.allCategories();
    setCategories(cats);
  }, [setBook, setCategories]);

  const handleOpen = useCallback(async () => {
    const selected = await openDialog({ directory: true, multiple: false, title: 'Open Syllepsis Book' });
    if (!selected || typeof selected !== 'string') return;
    let info = await api.openBook(selected);
    const warning = info.open_warning;

    if (warning?.should_offer_create_here) {
      const shouldCreate = confirm(
        `${missingBookFilesMessage(info)}\n\nCreate Syllepsis book files here? Cancel opens with in-memory defaults.`
      );
      if (shouldCreate) {
        const details = promptForBookCreationDetails(info.name);
        if (details) {
          try {
            info = await api.createBook(selected, details.name, details.language, details.location);
          } catch (error) {
            alert(`Could not create book files here: ${String(error)}`);
          }
        }
      }
    } else if (warning) {
      alert(`${missingBookFilesMessage(info)}\n\nOpening with in-memory defaults.`);
    }

    await finishOpeningBook(info);
  }, [finishOpeningBook]);

  const handleCreate = useCallback(async () => {
    const parentDir = await openDialog({
      directory: true,
      multiple: false,
      title: 'Choose where to create the new book folder',
    });
    if (!parentDir || typeof parentDir !== 'string') return;
    const details = promptForBookCreationDetails();
    if (!details) return;
    let info: BookInfo;
    try {
      info = await api.createBookInParent(parentDir, details.name, details.language, details.location);
    } catch (error) {
      alert(`Could not create book: ${String(error)}`);
      return;
    }
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
