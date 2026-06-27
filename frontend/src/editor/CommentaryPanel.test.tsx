import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
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

vi.mock('../lib/api', () => ({
  api: {
    listCommentary: vi.fn(async () => [summaryCommentary]),
    renderNoteMarkdown: vi.fn(async ({ markdown }: { markdown: string }) => `<p>${markdown}</p>`),
  },
}));

vi.mock('../components/Icon', () => ({
  Icon: ({ name }: { name: string }) => <span aria-hidden="true">{name}</span>,
}));

describe('CommentaryPanel', () => {
  it('opens focused summary proposal commentary when note lifecycle is omitted', async () => {
    const note = {
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

    render(
      <CommentaryPanel
        note={note}
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
    expect(screen.queryByRole('button', { name: 'Pin' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Dismiss' })).toBeNull();
  });
});
