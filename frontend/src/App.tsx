import { useCallback, useEffect, useState } from 'react';
import type { FormEvent, ReactNode } from 'react';
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
import type { BookInfo, TrackedBookInfo, ObjectType } from './types';
import './App.css';

const PACK_FILTER = [{ name: 'Syllepsis pack', extensions: ['synpack.json', 'json'] }];

function missingBookFilesMessage(info: BookInfo): string {
  const missingFiles = info.open_warning?.missing_reserved_files.join(', ') ?? '';
  return `This folder is missing ${missingFiles}, so it may not be a Syllepsis book.`;
}

// ──────────────────────────────────────────────
// First-launch screen: open or create a book
// ──────────────────────────────────────────────
function BookPicker() {
  const { setBook, setCategories } = useStore();
  const [trackedBooks, setTrackedBooks] = useState<TrackedBookInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyPath, setBusyPath] = useState<string | null>(null);
  const [mode, setMode] = useState<'create' | 'import' | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reloadTrackedBooks = useCallback(async () => {
    setLoading(true);
    try {
      setTrackedBooks(await api.listTrackedBooks());
      setError(null);
    } catch (error) {
      setError(String(error));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    reloadTrackedBooks();
  }, [reloadTrackedBooks]);

  const finishOpeningBook = useCallback(async (info: BookInfo) => {
    setBook(info);
    const cats = await api.allCategories();
    setCategories(cats);
  }, [setBook, setCategories]);

  const openExistingPath = useCallback(async (path: string) => {
    setBusyPath(path);
    setError(null);
    let info: BookInfo;
    try {
      info = await api.openBook(path);
    } catch (error) {
      setError(String(error));
      setBusyPath(null);
      await reloadTrackedBooks();
      return;
    }
    const warning = info.open_warning;

    if (warning) {
      alert(`${missingBookFilesMessage(info)}\n\nOpening with in-memory defaults.`);
    }

    await finishOpeningBook(info);
  }, [finishOpeningBook, reloadTrackedBooks]);

  const handleAddExisting = useCallback(async () => {
    const selected = await openDialog({ directory: true, multiple: false, title: 'Add Existing Syllepsis Book' });
    if (!selected || typeof selected !== 'string') return;
    await openExistingPath(selected);
  }, [openExistingPath]);

  const handleForget = useCallback(async (path: string) => {
    try {
      await api.forgetTrackedBook(path);
      await reloadTrackedBooks();
    } catch (error) {
      setError(String(error));
    }
  }, [reloadTrackedBooks]);

  const handleBookCreated = useCallback(async (info: BookInfo) => {
    await finishOpeningBook(info);
  }, [finishOpeningBook]);

  return (
    <div className="picker-root">
      <div className="picker-card">
        <div className="picker-logo">S</div>
        <h1 className="picker-title">Syllepsis</h1>
        <p className="picker-subtitle">Choose a local knowledge book.</p>
        {error && <div className="picker-error" onClick={() => setError(null)}>{error}</div>}

        <div className="tracked-books">
          {loading ? (
            <div className="picker-empty">Loading books...</div>
          ) : trackedBooks.length === 0 ? (
            <div className="picker-empty">No books are tracked on this device yet.</div>
          ) : (
            trackedBooks.map((book) => (
              <div key={book.path} className={`tracked-book-row ${book.available ? '' : 'unavailable'}`}>
                <button
                  className="tracked-book-main"
                  disabled={!book.available || busyPath === book.path}
                  onClick={() => openExistingPath(book.path)}
                >
                  <span className="tracked-book-name">{book.name}</span>
                  <span className="tracked-book-path">{book.path}</span>
                  {book.status && <span className="tracked-book-status">{book.status}</span>}
                </button>
                {!book.available && (
                  <button className="tracked-book-forget" onClick={() => handleForget(book.path)}>
                    Forget
                  </button>
                )}
              </div>
            ))
          )}
        </div>

        <div className="picker-actions">
          <button className="picker-btn picker-btn-primary" onClick={() => setMode('create')}>
            Create New Book
          </button>
          <button className="picker-btn picker-btn-secondary" onClick={() => setMode('import')}>
            Import Book
          </button>
          <button className="picker-btn picker-btn-secondary" onClick={handleAddExisting}>
            Add Existing Book...
          </button>
        </div>
      </div>

      {mode === 'create' && (
        <CreateBookWizard
          onCancel={() => setMode(null)}
          onCreated={handleBookCreated}
          onError={setError}
        />
      )}

      {mode === 'import' && (
        <ImportBookWizard
          onCancel={() => setMode(null)}
          onCreated={handleBookCreated}
          onError={setError}
        />
      )}
    </div>
  );
}

interface BookWizardProps {
  onCancel: () => void;
  onCreated: (info: BookInfo) => Promise<void>;
  onError: (message: string) => void;
}

function CreateBookWizard({ onCancel, onCreated, onError }: BookWizardProps) {
  const [name, setName] = useState('');
  const [language, setLanguage] = useState('en');
  const [location, setLocation] = useState('');
  const [parentPath, setParentPath] = useState('');
  const [busy, setBusy] = useState(false);

  const chooseParent = useCallback(async () => {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: 'Choose where to create the new book folder',
    });
    if (selected && typeof selected === 'string') setParentPath(selected);
  }, []);

  const submit = useCallback(async (event: FormEvent) => {
    event.preventDefault();
    if (!name.trim() || !parentPath) return;
    setBusy(true);
    try {
      const info = await api.createBookInParent(
        parentPath,
        name.trim(),
        language.trim() || undefined,
        location.trim() || undefined,
      );
      await onCreated(info);
    } catch (error) {
      onError(String(error));
      setBusy(false);
    }
  }, [language, location, name, onCreated, onError, parentPath]);

  return (
    <WizardShell title="Create New Book" onCancel={onCancel}>
      <form className="wizard-form" onSubmit={submit}>
        <label className="wizard-field">
          <span>Book name</span>
          <input value={name} onChange={(event) => setName(event.target.value)} autoFocus />
        </label>
        <label className="wizard-field">
          <span>Primary language</span>
          <input value={language} onChange={(event) => setLanguage(event.target.value)} />
        </label>
        <label className="wizard-field">
          <span>Default location (optional)</span>
          <input
            value={location}
            onChange={(event) => setLocation(event.target.value)}
            placeholder={'e.g. "London" or "51.5,-0.13"'}
          />
          <span className="wizard-hint">
            A place name or coordinates used to pin notes on a map — not where the book is saved.
          </span>
        </label>
        <label className="wizard-field">
          <span>Parent folder</span>
          <div className="path-picker">
            <input value={parentPath} readOnly placeholder="Choose a folder" />
            <button type="button" className="picker-btn picker-btn-secondary" onClick={chooseParent}>
              Choose...
            </button>
          </div>
        </label>
        <div className="wizard-actions">
          <button type="button" className="picker-btn picker-btn-secondary" onClick={onCancel}>
            Cancel
          </button>
          <button type="submit" className="picker-btn picker-btn-primary" disabled={busy || !name.trim() || !parentPath}>
            Create Book
          </button>
        </div>
      </form>
    </WizardShell>
  );
}

function ImportBookWizard({ onCancel, onCreated, onError }: BookWizardProps) {
  const [packPath, setPackPath] = useState('');
  const [parentPath, setParentPath] = useState('');
  const [bookName, setBookName] = useState('');
  const [busy, setBusy] = useState(false);

  const choosePack = useCallback(async () => {
    const selected = await openDialog({
      multiple: false,
      title: 'Choose exported book pack',
      filters: PACK_FILTER,
    });
    if (!selected || typeof selected !== 'string') return;
    setBusy(true);
    try {
      const manifest = await api.readPackManifest(selected);
      setPackPath(selected);
      setBookName((currentName) => currentName.trim() || manifest.name);
    } catch (error) {
      onError(String(error));
    } finally {
      setBusy(false);
    }
  }, [onError]);

  const chooseParent = useCallback(async () => {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: 'Choose where to create the imported book folder',
    });
    if (selected && typeof selected === 'string') setParentPath(selected);
  }, []);

  const submit = useCallback(async (event: FormEvent) => {
    event.preventDefault();
    if (!packPath || !parentPath || !bookName.trim()) return;
    setBusy(true);
    try {
      const info = await api.importPackAsBook(packPath, parentPath, bookName.trim());
      await onCreated(info);
    } catch (error) {
      onError(String(error));
      setBusy(false);
    }
  }, [bookName, onCreated, onError, packPath, parentPath]);

  return (
    <WizardShell title="Import Book" onCancel={onCancel}>
      <form className="wizard-form" onSubmit={submit}>
        <label className="wizard-field">
          <span>Pack file</span>
          <div className="path-picker">
            <input value={packPath} readOnly placeholder="Choose a .synpack.json file" />
            <button type="button" className="picker-btn picker-btn-secondary" onClick={choosePack}>
              Choose...
            </button>
          </div>
        </label>
        <label className="wizard-field">
          <span>Book name</span>
          <input value={bookName} onChange={(event) => setBookName(event.target.value)} />
        </label>
        <label className="wizard-field">
          <span>Parent folder</span>
          <div className="path-picker">
            <input value={parentPath} readOnly placeholder="Choose a folder" />
            <button type="button" className="picker-btn picker-btn-secondary" onClick={chooseParent}>
              Choose...
            </button>
          </div>
        </label>
        <div className="wizard-actions">
          <button type="button" className="picker-btn picker-btn-secondary" onClick={onCancel}>
            Cancel
          </button>
          <button type="submit" className="picker-btn picker-btn-primary" disabled={busy || !packPath || !parentPath || !bookName.trim()}>
            Import Book
          </button>
        </div>
      </form>
    </WizardShell>
  );
}

function WizardShell({ title, onCancel, children }: { title: string; onCancel: () => void; children: ReactNode }) {
  return (
    <div className="wizard-backdrop" role="presentation">
      <section className="wizard-panel" role="dialog" aria-modal="true" aria-labelledby="wizard-title">
        <div className="wizard-header">
          <h2 id="wizard-title">{title}</h2>
          <button className="wizard-close" onClick={onCancel} aria-label="Close">
            x
          </button>
        </div>
        {children}
      </section>
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

  const handleNewNote = useCallback(async (type: ObjectType = 'note') => {
    const note = await api.createNote(type, 'New Note');
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
