import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { Editor } from './Editor';
import { useStore } from '../lib/store';
import type { NoteDto } from '../types';

const mocks = vi.hoisted(() => ({
  getNote: vi.fn<() => Promise<NoteDto>>(),
  createCommentary: vi.fn(async () => ({ id: 'commentary-1' })),
}));

vi.mock('../lib/api', () => ({
  api: {
    getNote: mocks.getNote,
    listNotes: vi.fn(async () => []),
    noteNeighbors: vi.fn(async () => ({})),
    noteEmbeddingDetails: vi.fn(async () => null),
    listCommentary: vi.fn(async () => []),
    policyOverview: vi.fn(async () => ({ unlock_delay_hours: 24 })),
    noteSyncActivity: vi.fn(async () => null),
    noteEditingFinished: vi.fn(async () => {}),
    createCommentary: mocks.createCommentary,
    enqueueLlmJob: vi.fn(async () => ({})),
    renderNoteMarkdown: vi.fn(async ({ markdown }: { markdown: string }) => `<p>${markdown}</p>`),
    allCategories: vi.fn(async () => []),
    updateNote: vi.fn(async (note: NoteDto) => note),
  },
}));

vi.mock('../components/Icon', () => ({
  Icon: ({ name }: { name: string }) => <span aria-hidden="true">{name}</span>,
}));

vi.mock('./MetaPanel', () => ({
  MetaPanel: () => null,
}));

vi.mock('./LlmToolsMenu', () => ({
  LlmToolsMenu: () => null,
}));

vi.mock('./CommentaryPanel', () => ({
  CommentaryPanel: () => <div data-testid="commentary-panel" />,
}));

vi.mock('./DrawingEditor', () => ({
  DrawingEditor: () => null,
}));

vi.mock('../components/RelatedCarousel', () => ({
  RelatedCarousel: ({
    collapsed,
    onCollapsedChange,
  }: {
    collapsed?: boolean;
    onCollapsedChange?: (collapsed: boolean) => void;
  }) => (
    <button
      type="button"
      data-testid="related-carousel"
      aria-pressed={collapsed ?? false}
      onClick={() => onCollapsedChange?.(!(collapsed ?? false))}
    >
      {collapsed ? 'related collapsed' : 'related open'}
    </button>
  ),
}));

vi.mock('../components/MarkdownRenderer', () => ({
  MarkdownRenderer: ({ markdown }: { markdown: string }) => <div>{markdown}</div>,
}));

function lockedNote(): NoteDto {
  return {
    id: 'note-locked-01j00000000000000000000000',
    type: 'note',
    title: 'Locked',
    summary: '',
    body: 'baseline body',
    categories: [],
    sorted: false,
    metadata: {
      classification: {
        kind: 'note',
        basis: 'none',
        checkability: 'none',
        stability: 'settled',
        priority: 'standard',
        starred: false,
        stylistic_elements: [],
      },
      dates: {
        created: '2026-07-07T00:00:00Z',
        updated: '2026-07-07T00:00:00Z',
      },
      authorship: {},
      lifecycle: { lock: 'unlock_delay' },
      packs: {},
      kanban: {},
    },
  };
}

function unlockedNote(): NoteDto {
  return {
    ...lockedNote(),
    id: 'note-unlocked-01j00000000000000000000000',
    title: 'Unlocked',
    metadata: {
      ...lockedNote().metadata,
      lifecycle: {},
    },
  };
}

function installLocalStorageStub() {
  const values = new Map<string, string>();
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
    removeItem: (key: string) => values.delete(key),
    clear: () => values.clear(),
    key: (index: number) => [...values.keys()][index] ?? null,
    get length() {
      return values.size;
    },
  });
}

describe('Editor locked-note drafts', () => {
  beforeEach(() => {
    cleanup();
    installLocalStorageStub();
    localStorage.clear();
    vi.clearAllMocks();
    useStore.setState({
      editorFocusMode: false,
      desktopSidebarCollapsed: false,
      sidebarOpen: false,
      pluginsLoaded: true,
    });
    mocks.getNote.mockResolvedValue(lockedNote());
  });

  it('restores a locked source draft after unmounting and reopening', async () => {
    const first = render(<Editor noteId={lockedNote().id} initialMode="source" />);
    const textarea = await screen.findByDisplayValue('baseline body');

    fireEvent.change(textarea, { target: { value: 'unsaved proposal draft' } });
    first.unmount();

    render(<Editor noteId={lockedNote().id} initialMode="source" />);

    expect(await screen.findByDisplayValue('unsaved proposal draft')).toBeTruthy();
    expect(screen.getByRole('button', { name: /submit proposal/i })).toBeTruthy();
  });

  it('submits the latest locked source draft and clears the stored draft', async () => {
    render(<Editor noteId={lockedNote().id} initialMode="source" />);
    const textarea = await screen.findByDisplayValue('baseline body');

    fireEvent.change(textarea, { target: { value: 'proposal body now' } });
    fireEvent.click(screen.getByRole('button', { name: /submit proposal/i }));

    await waitFor(() => {
      expect(mocks.createCommentary).toHaveBeenCalledWith(
        lockedNote().id,
        'proposal',
        'proposal body now',
      );
    });

    cleanup();
    render(<Editor noteId={lockedNote().id} initialMode="source" />);
    expect(await screen.findByDisplayValue('baseline body')).toBeTruthy();
  });
});

describe('Editor focus mode layout controls', () => {
  beforeEach(() => {
    cleanup();
    installLocalStorageStub();
    localStorage.clear();
    vi.clearAllMocks();
    useStore.setState({
      editorFocusMode: false,
      desktopSidebarCollapsed: false,
      sidebarOpen: true,
      pluginsLoaded: true,
    });
    mocks.getNote.mockResolvedValue(unlockedNote());
  });

  it('collapses related notes and both sidebar modes when entering focus mode', async () => {
    render(<Editor noteId={unlockedNote().id} initialMode="read" />);

    expect((await screen.findByTestId('related-carousel')).textContent).toBe('related open');

    fireEvent.click(screen.getByTitle('Focus mode'));

    expect(useStore.getState().editorFocusMode).toBe(true);
    expect(useStore.getState().desktopSidebarCollapsed).toBe(true);
    expect(useStore.getState().sidebarOpen).toBe(false);
    expect(screen.getByTestId('related-carousel').textContent).toBe('related collapsed');
  });

  it('expands the desktop sidebar on exit without reopening mobile sidebar or changing related notes', async () => {
    render(<Editor noteId={unlockedNote().id} initialMode="read" />);

    const relatedCarousel = await screen.findByTestId('related-carousel');
    fireEvent.click(relatedCarousel);
    expect(screen.getByTestId('related-carousel').textContent).toBe('related collapsed');

    fireEvent.click(screen.getByTitle('Focus mode'));
    expect(screen.getByTestId('related-carousel').textContent).toBe('related collapsed');

    fireEvent.click(screen.getByTitle('Exit focus mode'));

    expect(useStore.getState().editorFocusMode).toBe(false);
    expect(useStore.getState().desktopSidebarCollapsed).toBe(false);
    expect(useStore.getState().sidebarOpen).toBe(false);
    expect(screen.getByTestId('related-carousel').textContent).toBe('related collapsed');
  });
});
