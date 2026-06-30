import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { FlexDate, NoteDto, NoteStatus } from '../types';
import { MetaPanel } from './MetaPanel';

vi.mock('../lib/api', () => ({
  api: {
    setNoteLock: vi.fn(async () => undefined),
  },
}));

vi.mock('../components/Icon', () => ({
  Icon: ({ name }: { name: string }) => <span aria-hidden="true">{name}</span>,
}));

vi.mock('../components/WorldLocationHelper', () => ({
  WorldLocationHelper: () => null,
}));

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date('2026-06-30T12:00:00'));
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('MetaPanel workflow status dates', () => {
  it('stamps the started date when status is marked active', () => {
    const onChange = vi.fn();
    renderMetaPanel({ onChange });

    openWorkflowSection();
    fireEvent.click(screen.getByRole('button', { name: /^active$/i }));

    const updated = onChange.mock.calls[0][0] as NoteDto;
    expect(updated.metadata.status).toBe('active');
    expect(updated.metadata.dates.started).toEqual({ date: '2026-06-30' });
    expect(updated.metadata.dates.completed).toBeUndefined();
  });

  it('stamps the completed date when status is marked done without replacing an existing date', () => {
    const onChange = vi.fn();
    renderMetaPanel({ onChange });

    openWorkflowSection();
    fireEvent.click(screen.getByRole('button', { name: /^done$/i }));

    const updated = onChange.mock.calls[0][0] as NoteDto;
    expect(updated.metadata.status).toBe('done');
    expect(updated.metadata.dates.completed).toEqual({ date: '2026-06-30' });

    onChange.mockClear();
    cleanup();
    renderMetaPanel({
      note: noteDto({
        status: 'open',
        completed: { date: '2026-05-01' },
      }),
      onChange,
    });

    openWorkflowSection();
    fireEvent.click(screen.getByRole('button', { name: /^done$/i }));

    const updatedWithExistingDate = onChange.mock.calls[0][0] as NoteDto;
    expect(updatedWithExistingDate.metadata.status).toBe('done');
    expect(updatedWithExistingDate.metadata.dates.completed).toEqual({ date: '2026-05-01' });
  });
});

function renderMetaPanel({
  note = noteDto(),
  onChange = vi.fn(),
}: {
  note?: NoteDto;
  onChange?: (next: NoteDto) => void;
} = {}) {
  return render(
    <MetaPanel
      note={note}
      categories={[]}
      allNotes={[]}
      embeddingDetails={null}
      onChange={onChange}
    />,
  );
}

function openWorkflowSection() {
  fireEvent.click(screen.getByRole('button', { name: /details & metadata/i }));
  fireEvent.click(screen.getByRole('button', { name: /workflow/i }));
}

function noteDto({
  status,
  started,
  completed,
}: {
  status?: NoteStatus;
  started?: FlexDate;
  completed?: FlexDate;
} = {}): NoteDto {
  return {
    id: 'note-1',
    type: 'note',
    title: 'Task One',
    summary: 'Summary',
    body: 'Body',
    categories: [],
    sorted: false,
    metadata: {
      status,
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
        created: '2026-06-01T00:00:00Z',
        updated: '2026-06-01T00:00:00Z',
        started,
        completed,
      },
      authorship: {},
      packs: {},
      kanban: {},
    },
  };
}
