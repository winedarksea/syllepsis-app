import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CommentaryPanel } from './CommentaryPanel';
import type { CommentarySummary, NoteDto } from '../types';

const summaryCommentary: CommentarySummary = {
  id: 'commentary-1',
  title: 'summarize proposal for Parent',
  body: 'A concise generated summary.',
  metadata: {
    parent_note_id: 'note-1',
    kind: 'proposal',
    status: 'open',
    source: 'ai',
    target_field: 'summary',
    task: 'summarize',
  },
  created: '2026-06-26T00:00:00Z',
  updated: '2026-06-26T00:00:00Z',
};

const mocks = vi.hoisted(() => ({
  listCommentary: vi.fn(async () => [summaryCommentary]),
  pinCommentary: vi.fn(async () => ({ id: 'commentary-1' })),
  dismissCommentary: vi.fn(async () => ({ id: 'commentary-1' })),
}));

vi.mock('../lib/api', () => ({
  api: {
    listCommentary: mocks.listCommentary,
    pinCommentary: mocks.pinCommentary,
    dismissCommentary: mocks.dismissCommentary,
    renderNoteMarkdown: vi.fn(async ({ markdown }: { markdown: string }) => `<p>${markdown}</p>`),
  },
}));

vi.mock('../components/Icon', () => ({
  Icon: ({ name }: { name: string }) => <span aria-hidden="true">{name}</span>,
}));

function note(): NoteDto {
  return {
    id: 'note-1',
    type: 'note',
    title: 'Parent',
    summary: '',
    body: 'Parent body',
    categories: [],
    sorted: false,
    metadata: {
      classification: {
        statement_type: 'idea',
        basis: 'none',
        checkability: 'none',
        stability: 'settled',
        priority: 'standard',
        starred: false,
        stylistic_elements: [],
      },
      dates: {
        created: '2026-06-26T00:00:00Z',
        updated: '2026-06-26T00:00:00Z',
      },
      authorship: {},
      packs: {},
      kanban: {},
    },
  } as NoteDto;
}

describe('CommentaryPanel', () => {
  beforeEach(() => {
    cleanup();
    vi.clearAllMocks();
    mocks.listCommentary.mockResolvedValue([summaryCommentary]);
  });

  it('opens focused summary proposal commentary when note lifecycle is omitted', async () => {
    render(
      <CommentaryPanel
        note={note()}
        currentBody="Parent body"
        focusId="commentary-1"
        onClose={() => {}}
        onApplied={() => {}}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole('dialog')).not.toBeNull();
    });
    expect(screen.getAllByText('A concise generated summary.').length).toBeGreaterThan(0);
    expect(screen.getByRole('button', { name: 'Apply' })).not.toBeNull();
    expect(screen.getByRole('button', { name: 'Back' })).not.toBeNull();
    expect(screen.getByRole('button', { name: 'Pin' })).not.toBeNull();
    expect(screen.getByRole('button', { name: 'Delete' })).not.toBeNull();
  });

  it('does not reopen focused commentary after pinning it', async () => {
    const pinnedCommentary = {
      ...summaryCommentary,
      metadata: {
        ...summaryCommentary.metadata,
        status: 'pinned' as const,
      },
    };
    mocks.listCommentary
      .mockResolvedValueOnce([summaryCommentary])
      .mockResolvedValueOnce([pinnedCommentary]);

    render(
      <CommentaryPanel
        note={note()}
        currentBody="Parent body"
        focusId="commentary-1"
        onClose={() => {}}
        onApplied={() => {}}
      />,
    );

    await waitFor(() => expect(screen.getByRole('dialog')).not.toBeNull());
    fireEvent.click(screen.getByRole('button', { name: 'Pin' }));

    await waitFor(() => {
      expect(mocks.pinCommentary).toHaveBeenCalledWith('commentary-1');
      expect(screen.queryByRole('dialog')).toBeNull();
    });
  });

  it('labels destructive pinned commentary action as delete and offers back', async () => {
    mocks.listCommentary.mockResolvedValue([
      {
        ...summaryCommentary,
        metadata: {
          ...summaryCommentary.metadata,
          kind: 'footnote',
          status: 'pinned',
        },
      },
    ]);

    render(
      <CommentaryPanel
        note={note()}
        currentBody="Parent body"
        focusId="commentary-1"
        onClose={() => {}}
        onApplied={() => {}}
      />,
    );

    await waitFor(() => expect(screen.getByRole('dialog')).not.toBeNull());
    expect(screen.getByRole('button', { name: 'Back' })).not.toBeNull();
    expect(screen.getByRole('button', { name: 'Delete' })).not.toBeNull();
    expect(screen.queryByRole('button', { name: 'Pin' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Dismiss' })).toBeNull();
  });
});
