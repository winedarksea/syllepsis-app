import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import type { ClassificationKind, NoteDto } from '../types';
import { UnsortedQueue } from './UnsortedQueue';

vi.mock('../lib/api', () => ({
  api: {
    listNotes: vi.fn(),
    unsortedNotes: vi.fn(),
  },
}));

beforeEach(() => {
  useStore.setState({
    view: 'unsorted',
    unsortedCount: 0,
    editingNoteId: null,
  });
  vi.mocked(api.listNotes).mockReset();
  vi.mocked(api.unsortedNotes).mockReset();
});

afterEach(() => {
  cleanup();
});

describe('UnsortedQueue', () => {
  it('renders the simplified topbar without duplicate creation or lifecycle mode controls', async () => {
    const activeNote = noteDto({ id: 'active-unsorted', title: 'Active Unsorted Note', sorted: false });
    mockNotes({ activeNotes: [activeNote], activeUnsortedNotes: [activeNote] });

    render(<UnsortedQueue />);

    await waitFor(() => expect(api.listNotes).toHaveBeenCalledWith('active'));
    expect(await screen.findByText('Active Unsorted Note')).toBeTruthy();
    expect(screen.queryByText(/^New Note$/i)).toBeNull();
    expect(screen.queryByText(/Capture a thought/i)).toBeNull();
    expect(screen.queryByRole('button', { name: /^Active$/i })).toBeNull();
    expect(screen.queryByRole('button', { name: /^Archived$/i })).toBeNull();
    expect(screen.queryByRole('button', { name: /^Trash$/i })).toBeNull();
    // Classification select lives inside the Filter popover, hidden until opened.
    expect(screen.queryByLabelText('Classification')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: /^Filter/ }));
    expect(screen.getByLabelText('Classification')).toBeTruthy();
    expect(api.listNotes).not.toHaveBeenCalledWith('trash');
  });

  it('filters by classification from the unlabeled classification select', async () => {
    mockNotes({
      activeNotes: [
        noteDto({ id: 'note-kind', title: 'Plain Note', classification: 'note', sorted: false }),
        noteDto({ id: 'quote-kind', title: 'Quote Note', classification: 'quote', sorted: false }),
      ],
      activeUnsortedNotes: [],
    });

    render(<UnsortedQueue />);

    expect(await screen.findByText('Plain Note')).toBeTruthy();
    expect(await screen.findByText('Quote Note')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: /^Filter/ }));
    fireEvent.change(screen.getByLabelText('Classification'), { target: { value: 'quote' } });

    expect(screen.queryByText('Plain Note')).toBeNull();
    expect(screen.getByText('Quote Note')).toBeTruthy();
  });

  it('includes archived notes in all Notebox modes without requesting trash', async () => {
    const activeSortedNote = noteDto({
      id: 'active-sorted',
      title: 'Active Sorted Note',
      sorted: true,
      categories: ['projects'],
    });
    const archivedUnsortedNote = noteDto({
      id: 'archived-unsorted',
      title: 'Archived Unsorted Note',
      sorted: false,
      archived: true,
      categories: [],
    });
    mockNotes({
      activeNotes: [activeSortedNote],
      archivedNotes: [archivedUnsortedNote],
      activeUnsortedNotes: [],
    });

    render(<UnsortedQueue />);

    expect(await screen.findByText('All caught up! Every note has been organised.')).toBeTruthy();
    expect(api.listNotes).not.toHaveBeenCalledWith('archived');

    fireEvent.click(screen.getByRole('button', { name: /^Filter/ }));
    fireEvent.click(screen.getByLabelText('Include archived'));

    await waitFor(() => expect(api.listNotes).toHaveBeenCalledWith('archived'));
    expect(screen.getByText('Archived Unsorted Note')).toBeTruthy();
    expect(within(screen.getByText('Archived Unsorted Note').closest('.uq-card')!).getByText('Archived')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'All notes' }));
    expect(screen.getByText('Active Sorted Note')).toBeTruthy();
    expect(screen.getByText('Archived Unsorted Note')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Uncategorized' }));
    expect(screen.queryByText('Active Sorted Note')).toBeNull();
    expect(screen.getByText('Archived Unsorted Note')).toBeTruthy();
    expect(api.listNotes).not.toHaveBeenCalledWith('trash');
  });

  it('includes hidden notes when Include private is toggled in the Filter popover', async () => {
    const privateNote = noteDto({ id: 'private-note', title: 'Private Note', sorted: false, hidden: true });
    mockNotes({ activeNotes: [], privateNotes: [privateNote], activeUnsortedNotes: [] });

    render(<UnsortedQueue />);

    await waitFor(() => expect(api.listNotes).toHaveBeenCalledWith('active'));
    expect(api.listNotes).not.toHaveBeenCalledWith('hidden');
    expect(screen.queryByText('Private Note')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: /^Filter/ }));
    fireEvent.click(screen.getByLabelText('Include private'));

    await waitFor(() => expect(api.listNotes).toHaveBeenCalledWith('hidden'));
    expect(await screen.findByText('Private Note')).toBeTruthy();
    expect(within(screen.getByText('Private Note').closest('.uq-card')!).getByText('Private')).toBeTruthy();

    // Active-filter count chip reflects the single active filter.
    expect(screen.getByRole('button', { name: /^Filter · 1/ })).toBeTruthy();
  });
});

function mockNotes({
  activeNotes,
  archivedNotes = [],
  privateNotes = [],
  activeUnsortedNotes,
}: {
  activeNotes: NoteDto[];
  archivedNotes?: NoteDto[];
  privateNotes?: NoteDto[];
  activeUnsortedNotes: NoteDto[];
}) {
  vi.mocked(api.listNotes).mockImplementation(async (visibility) => (
    visibility === 'archived' ? archivedNotes : visibility === 'hidden' ? privateNotes : activeNotes
  ));
  vi.mocked(api.unsortedNotes).mockResolvedValue(activeUnsortedNotes);
}

function noteDto({
  id,
  title,
  classification = 'note',
  sorted = false,
  archived = false,
  hidden = false,
  categories = [],
}: {
  id: string;
  title: string;
  classification?: ClassificationKind;
  sorted?: boolean;
  archived?: boolean;
  hidden?: boolean;
  categories?: string[];
}): NoteDto {
  return {
    id,
    type: 'note',
    title,
    summary: '',
    body: '',
    categories,
    sorted,
    metadata: {
      classification: {
        kind: classification,
        basis: 'none',
        checkability: 'none',
        stability: 'evolving',
        priority: 'standard',
        starred: false,
        stylistic_elements: [],
      },
      dates: {
        created: '2024-01-01T00:00:00Z',
        updated: '2024-01-02T00:00:00Z',
      },
      authorship: {},
      lifecycle: { ...(archived ? { archived: true } : {}), ...(hidden ? { hidden: true } : {}) },
      packs: {},
      kanban: {},
    },
  };
}
