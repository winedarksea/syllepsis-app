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

describe('Sidebar desktop collapse controls', () => {
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

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it('fires the desktop collapse callback from the header control', () => {
    const onDesktopCollapse = vi.fn();

    render(
      <Sidebar
        onNewNote={vi.fn()}
        onImportImage={vi.fn()}
        onNewDrawing={vi.fn()}
        onDesktopCollapse={onDesktopCollapse}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /collapse navigation/i }));

    expect(onDesktopCollapse).toHaveBeenCalledTimes(1);
  });

  it('hides sidebar controls from assistive and pointer interaction when desktop-collapsed', () => {
    const { container } = render(
      <Sidebar
        onNewNote={vi.fn()}
        onImportImage={vi.fn()}
        onNewDrawing={vi.fn()}
        isDesktopCollapsed
      />,
    );

    const sidebar = container.querySelector('aside');
    expect(sidebar?.getAttribute('aria-hidden')).toBe('true');
    expect(sidebar?.hasAttribute('inert')).toBe(true);
  });
});
