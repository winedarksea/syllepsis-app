import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LlmJobTray } from './LlmJobTray';
import { useStore } from '../lib/store';
import type { QueuedLlmJobResult } from '../types';

const proposal = {
  id: 'proposal-1',
  target: 'note-1',
  task: 'rewrite' as const,
  provider: 'local',
  model: 'gemma',
  live: false,
  content: 'new body',
  status: 'pending' as const,
  created_at: '2026-07-07T00:00:00Z',
};

const mocks = vi.hoisted(() => ({
  listLlmJobs: vi.fn<() => Promise<QueuedLlmJobResult[]>>(),
  dismissLlmJobResult: vi.fn(async () => {}),
}));

vi.mock('../lib/api', () => ({
  api: {
    listLlmJobs: mocks.listLlmJobs,
    listAllLlmJobs: vi.fn(async () => []),
    dismissLlmJobResult: mocks.dismissLlmJobResult,
  },
}));

vi.mock('./Icon', () => ({
  Icon: ({ name }: { name: string }) => <span aria-hidden="true">{name}</span>,
}));

describe('LlmJobTray', () => {
  beforeEach(() => {
    cleanup();
    vi.clearAllMocks();
    useStore.setState({
      view: 'unsorted',
      editingNoteId: null,
      editingMode: 'read',
      editorReturnView: null,
      commentaryFocusId: null,
    });
  });

  it('opens commentary results at the focused commentary id', async () => {
    mocks.listLlmJobs.mockResolvedValue([
      {
        job_id: '01j00000000000000000000000',
        status: 'complete',
        target_note_id: 'note-1',
        task: 'rewrite',
        proposal,
        commentary_id: 'commentary-1',
        error: null,
      },
    ]);

    render(<LlmJobTray />);

    fireEvent.click(await screen.findByRole('button', { name: /rewrite/i }));

    await waitFor(() => {
      expect(useStore.getState().view).toBe('editor');
      expect(useStore.getState().editingNoteId).toBe('note-1');
      expect(useStore.getState().commentaryFocusId).toBe('commentary-1');
    });
  });

  it('keeps proposal-only completed jobs visible and opens the target note', async () => {
    mocks.listLlmJobs.mockResolvedValue([
      {
        job_id: '01j00000000000000000000001',
        status: 'complete',
        target_note_id: 'note-2',
        task: 'rewrite',
        proposal: { ...proposal, id: 'proposal-2', target: 'note-2' },
        commentary_id: null,
        error: null,
      },
    ]);

    render(<LlmJobTray />);

    fireEvent.click(await screen.findByRole('button', { name: /rewrite/i }));

    await waitFor(() => {
      expect(useStore.getState().view).toBe('editor');
      expect(useStore.getState().editingNoteId).toBe('note-2');
      expect(useStore.getState().commentaryFocusId).toBeNull();
    });
  });
});
