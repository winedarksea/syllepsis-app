import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { TrashView } from './TrashView';
import { useStore } from '../lib/store';
import type { PendingDeletion, PolicyOverview } from '../types';

const apiMocks = vi.hoisted(() => ({
  policyOverview: vi.fn(),
  getBookConfig: vi.fn(),
  restoreNote: vi.fn(),
  purgeExpired: vi.fn(),
  purgeAllTrash: vi.fn(),
}));

vi.mock('../lib/api', () => ({
  api: {
    policyOverview: apiMocks.policyOverview,
    getBookConfig: apiMocks.getBookConfig,
    restoreNote: apiMocks.restoreNote,
    purgeExpired: apiMocks.purgeExpired,
    purgeAllTrash: apiMocks.purgeAllTrash,
  },
}));

vi.mock('../components/Icon', () => ({
  Icon: ({ name }: { name: string }) => <span aria-hidden="true">{name}</span>,
}));

function pending(id: string, title: string): PendingDeletion {
  return {
    id,
    title,
    marked_at: '2026-07-01T00:00:00Z',
    purge_at: '2026-07-30T00:00:00Z',
  };
}

function overview(pendingDeletion: PendingDeletion[]): PolicyOverview {
  return {
    hidden_notes: [],
    search_excluded_notes: [],
    publish_excluded_notes: [],
    archived_notes: [],
    locked_notes: [],
    pending_deletion: pendingDeletion,
    hidden_categories: [],
    search_excluded_categories: [],
    publish_excluded_categories: [],
    unlock_delay_hours: 24,
  };
}

beforeEach(() => {
  useStore.setState({ openEditor: vi.fn() });
  apiMocks.policyOverview.mockResolvedValue(overview([pending('n1', 'Doomed Note')]));
  apiMocks.getBookConfig.mockResolvedValue({ cleanup: { deletion_delay_days: 14 } });
  apiMocks.restoreNote.mockResolvedValue({});
  apiMocks.purgeExpired.mockResolvedValue(['n1']);
  apiMocks.purgeAllTrash.mockResolvedValue(['n1']);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('TrashView', () => {
  it('restores a pending-deletion note via restoreNote', async () => {
    render(<TrashView />);

    expect(await screen.findByText('Doomed Note')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Restore' }));

    await waitFor(() => expect(apiMocks.restoreNote).toHaveBeenCalledWith('n1'));
  });

  it('sweeps expired notes via purgeExpired', async () => {
    render(<TrashView />);
    await screen.findByText('Doomed Note');

    fireEvent.click(screen.getByRole('button', { name: 'Sweep now' }));

    await waitFor(() => expect(apiMocks.purgeExpired).toHaveBeenCalledTimes(1));
  });

  it('requires confirmation before deleting the whole trash', async () => {
    render(<TrashView />);
    await screen.findByText('Doomed Note');

    fireEvent.click(screen.getByRole('button', { name: 'Delete immediately' }));
    expect(apiMocks.purgeAllTrash).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    await waitFor(() => expect(apiMocks.purgeAllTrash).toHaveBeenCalledTimes(1));
  });

  it('shows an empty state with the configured delay when trash is empty', async () => {
    apiMocks.policyOverview.mockResolvedValue(overview([]));
    render(<TrashView />);

    expect(await screen.findByText('Trash is empty.')).toBeTruthy();
    expect(screen.getByText(/14 days/)).toBeTruthy();
  });
});
