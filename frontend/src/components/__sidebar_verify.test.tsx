import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Sidebar } from './Sidebar';
import { useStore } from '../lib/store';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => vi.fn()),
}));

vi.mock('../lib/api', () => ({
  api: {
    cloudSyncProviderStatuses: vi.fn(async () => []),
    gitStatus: vi.fn(async () => ({ is_repository: false })),
    operationalActivitySummary: vi.fn(async () => ({})),
  },
}));

vi.mock('./Icon', () => ({
  Icon: ({ name }: { name: string }) => <span aria-hidden="true">{name}</span>,
}));

describe('verify header cleanup', () => {
  beforeEach(() => {
    useStore.setState({
      view: 'unsorted',
      categories: [],
      unsortedCount: 0,
      hideUnsortedBadge: false,
      diagnosticsIssueCount: 0,
      activeCategory: null,
      desktopSidebarCollapsed: false,
    });
  });
  afterEach(() => { cleanup(); vi.clearAllMocks(); });

  it('renders the new header shape and wordmark click behavior', () => {
    const closeBook = vi.fn();
    useStore.setState({ closeBook });

    const { container } = render(
      <Sidebar onNewNote={vi.fn()} onImportImage={vi.fn()} onNewDrawing={vi.fn()} />,
    );

    const header = container.querySelector('.sidebar-header')!;
    console.log('HEADER HTML:\n', header.innerHTML);

    const wordmark = screen.getByRole('button', { name: /close book/i });
    expect(wordmark.tagName).toBe('BUTTON');
    expect(wordmark.textContent).toBe('Syllepsis');

    fireEvent.click(wordmark);
    expect(closeBook).toHaveBeenCalledTimes(1);

    // no more logout icon, no more theme toggle icon
    expect(screen.queryByText('logout')).toBeNull();
    expect(screen.queryByText('dark_mode')).toBeNull();
    expect(screen.queryByText('light_mode')).toBeNull();

    // settings + collapse still present
    expect(screen.getByRole('button', { name: 'Settings' })).toBeTruthy();
    expect(screen.getByRole('button', { name: /collapse navigation/i })).toBeTruthy();

    const headerActionButtons = header.querySelectorAll('.sidebar-header-actions > button');
    console.log('ACTION BUTTON COUNT (incl. mobile-close, hidden via CSS not DOM):', headerActionButtons.length);
  });
});
