import { cleanup, fireEvent, render, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import type { GraphAnalysisNode, NoteDto, NoteStatus } from '../types';
import { KanbanBoard } from './KanbanBoard';

vi.mock('../lib/api', () => ({
  api: {
    setNoteWorkflowStatus: vi.fn(),
  },
}));

const originalElementFromPoint = document.elementFromPoint;

beforeEach(() => {
  useStore.setState({
    book: { name: 'Test', path: '/tmp/test-book', open_warning: null },
    kanbanSelectedCategories: [],
    kanbanSelectedPriorities: ['standard', 'important', 'core'],
    kanbanShowNoStatus: true,
    kanbanColorBy: 'classification',
  });
  vi.mocked(api.setNoteWorkflowStatus).mockReset();
});

afterEach(() => {
  restoreElementFromPoint();
  cleanup();
});

describe('KanbanBoard', () => {
  it('opens a note from card click', () => {
    const onOpenNote = vi.fn();
    const { container } = renderBoard({ onOpenNote });

    fireEvent.click(container.querySelector('.kb-card')!);

    expect(onOpenNote).toHaveBeenCalledWith('note-1');
  });

  it('updates status from the card menu', async () => {
    const updated = noteDto('note-1', 'active');
    vi.mocked(api.setNoteWorkflowStatus).mockResolvedValue(updated);
    const onWorkflowUpdated = vi.fn();
    const { container, getByLabelText } = renderBoard({ onWorkflowUpdated });

    fireEvent.click(getByLabelText(/change status for task one/i));
    fireEvent.click(within(container.querySelector('.kb-status-menu')!).getByRole('button', { name: /^active$/i }));

    await waitFor(() => {
      expect(api.setNoteWorkflowStatus).toHaveBeenCalledWith('note-1', 'active', expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/));
    });
    expect(onWorkflowUpdated).toHaveBeenCalledWith(updated);
  });

  it('maps drag drops to open, active, and done statuses', async () => {
    vi.mocked(api.setNoteWorkflowStatus).mockImplementation(async (_id, status) => noteDto('note-1', status));
    const { container, getByLabelText } = renderBoard();
    const dragHandle = getByLabelText(/drag task one/i);
    const columns = Array.from(container.querySelectorAll('.kb-column'));

    for (const [index, expectedStatus] of ['open', 'active', 'done'].entries()) {
      const dataTransfer = {
        effectAllowed: '',
        setData: vi.fn(),
        getData: vi.fn(() => 'note-1'),
      };
      fireEvent.dragStart(dragHandle, { dataTransfer });
      fireEvent.drop(columns[index], { dataTransfer });
      await waitFor(() => {
        expect(api.setNoteWorkflowStatus).toHaveBeenCalledWith('note-1', expectedStatus, expect.any(String));
      });
    }
  });

  it('maps pointer dragging from the handle to the section under the pointer', async () => {
    vi.mocked(api.setNoteWorkflowStatus).mockImplementation(async (_id, status) => noteDto('note-1', status));
    const { container, getByLabelText } = renderBoard();
    const dragHandle = getByLabelText(/drag task one/i);
    const inProgressColumn = container.querySelectorAll('.kb-column')[1];
    mockElementFromPoint(inProgressColumn);

    fireEvent.pointerDown(dragHandle, {
      pointerId: 1,
      pointerType: 'touch',
      clientX: 10,
      clientY: 10,
    });
    fireEvent.pointerMove(dragHandle, {
      pointerId: 1,
      pointerType: 'touch',
      clientX: 40,
      clientY: 40,
    });
    fireEvent.pointerUp(dragHandle, {
      pointerId: 1,
      pointerType: 'touch',
      clientX: 40,
      clientY: 40,
    });

    await waitFor(() => {
      expect(api.setNoteWorkflowStatus).toHaveBeenCalledWith('note-1', 'active', expect.any(String));
    });
  });
});

function renderBoard(overrides: Partial<{
  onOpenNote: (id: string) => void;
  onWorkflowUpdated: (note: NoteDto) => void;
}> = {}) {
  return render(
    <KanbanBoard
      nodes={[graphNode('note-1', 'Task One', 'open')]}
      loading={false}
      onOpenNote={overrides.onOpenNote ?? vi.fn()}
      onWorkflowUpdated={overrides.onWorkflowUpdated ?? vi.fn()}
    />,
  );
}

function graphNode(id: string, title: string, status: NoteStatus | undefined): GraphAnalysisNode {
  return {
    id,
    type: 'note',
    title,
    summary: 'Summary',
    categories: ['work'],
    status,
    classification: 'todo',
    priority: 'standard',
    starred: false,
    created: '2024-01-01T00:00:00Z',
    updated: '2024-01-02T00:00:00Z',
    x: 0,
    y: 0,
    outlier: false,
    no_semantic_signal: false,
  };
}

function noteDto(id: string, status: NoteStatus | null): NoteDto {
  return {
    id,
    type: 'note',
    title: 'Task One',
    summary: 'Summary',
    body: 'Body',
    categories: ['work'],
    sorted: false,
    metadata: {
      status: status ?? undefined,
      classification: {
        kind: 'todo',
        basis: 'none',
        checkability: 'none',
        stability: 'evolving',
        priority: 'standard',
        starred: false,
        stylistic_elements: [],
      },
      dates: {
        created: '2024-01-01T00:00:00Z',
        updated: '2024-01-03T00:00:00Z',
        started: status === 'active' ? { date: '2024-01-03' } : undefined,
        completed: status === 'done' ? { date: '2024-01-03' } : undefined,
      },
      authorship: {},
      packs: {},
      kanban: {},
    },
  };
}

function mockElementFromPoint(element: Element) {
  Object.defineProperty(document, 'elementFromPoint', {
    configurable: true,
    value: vi.fn(() => element),
  });
}

function restoreElementFromPoint() {
  if (originalElementFromPoint) {
    Object.defineProperty(document, 'elementFromPoint', {
      configurable: true,
      value: originalElementFromPoint,
    });
  } else {
    Reflect.deleteProperty(document, 'elementFromPoint');
  }
}
