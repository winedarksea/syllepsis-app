import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
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
  listAllLlmJobs: vi.fn<() => Promise<QueuedLlmJobResult[]>>(async () => []),
  dismissLlmJobResult: vi.fn(async () => {}),
}));

vi.mock('../lib/api', () => ({
  api: {
    listLlmJobs: mocks.listLlmJobs,
    listAllLlmJobs: mocks.listAllLlmJobs,
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

describe('LlmJobTray polling gate', () => {
  const settledJob: QueuedLlmJobResult = {
    job_id: '01j00000000000000000000002',
    status: 'complete',
    target_note_id: 'note-3',
    task: 'rewrite',
    proposal: { ...proposal, id: 'proposal-3', target: 'note-3' },
    commentary_id: 'commentary-3',
    error: null,
  };
  const runningJob: QueuedLlmJobResult = {
    job_id: '01j00000000000000000000003',
    status: 'running',
    target_note_id: 'note-4',
    task: 'rewrite',
    proposal: null,
    commentary_id: null,
    error: null,
  };

  beforeEach(() => {
    cleanup();
    vi.clearAllMocks();
    vi.useFakeTimers();
    useStore.setState({ llmJobSubmittedSignal: 0 });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  // Lets the pending fetch promises resolve (and their state updates render) without letting any
  // interval fire.
  async function flushPendingFetches() {
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
  }

  it('does not poll after the interval elapses when no job is in flight', async () => {
    mocks.listLlmJobs.mockResolvedValue([settledJob]);

    render(<LlmJobTray />);
    await flushPendingFetches();
    const callsAfterInitialFetch = mocks.listLlmJobs.mock.calls.length;
    expect(callsAfterInitialFetch).toBe(1);

    await act(async () => { await vi.advanceTimersByTimeAsync(20_000); });

    expect(mocks.listLlmJobs.mock.calls.length).toBe(callsAfterInitialFetch);
  });

  it('keeps polling while a job is running', async () => {
    mocks.listLlmJobs.mockResolvedValue([runningJob]);

    render(<LlmJobTray />);
    await flushPendingFetches();
    const callsBeforeInterval = mocks.listLlmJobs.mock.calls.length;

    await act(async () => { await vi.advanceTimersByTimeAsync(7_600); });

    expect(mocks.listLlmJobs.mock.calls.length).toBeGreaterThan(callsBeforeInterval + 2);
  });

  it('fetches immediately when a job is submitted while the tray is idle', async () => {
    mocks.listLlmJobs.mockResolvedValue([settledJob]);

    render(<LlmJobTray />);
    await flushPendingFetches();
    expect(mocks.listLlmJobs).toHaveBeenCalledTimes(1);

    await act(async () => {
      useStore.getState().notifyLlmJobSubmitted();
      await vi.advanceTimersByTimeAsync(0);
    });

    expect(mocks.listLlmJobs).toHaveBeenCalledTimes(2);
  });

  it('fetches history on open and stops when its jobs are settled', async () => {
    mocks.listLlmJobs.mockResolvedValue([settledJob]);
    mocks.listAllLlmJobs.mockResolvedValue([settledJob]);

    render(<LlmJobTray />);
    await flushPendingFetches();
    expect(mocks.listAllLlmJobs).not.toHaveBeenCalled();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /history/i }));
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(mocks.listAllLlmJobs).toHaveBeenCalledTimes(1);

    await act(async () => { await vi.advanceTimersByTimeAsync(20_000); });

    expect(mocks.listAllLlmJobs).toHaveBeenCalledTimes(1);
  });
});
