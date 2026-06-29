import { cleanup, render, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MarkdownRenderer } from './MarkdownRenderer';

const mocks = vi.hoisted(() => ({
  renderNoteMarkdown: vi.fn(async ({ markdown }: { markdown?: string | null }) => `<p>${markdown ?? ''}</p>`),
}));

vi.mock('../lib/api', () => ({
  api: {
    renderNoteMarkdown: mocks.renderNoteMarkdown,
  },
}));

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn(),
}));

describe('MarkdownRenderer find highlighting', () => {
  beforeEach(() => {
    cleanup();
    vi.clearAllMocks();
    Element.prototype.scrollIntoView = vi.fn();
    mocks.renderNoteMarkdown.mockImplementation(async ({ markdown }: { markdown?: string | null }) => `<p>${markdown ?? ''}</p>`);
  });

  it('highlights literal case-insensitive matches in rendered text', async () => {
    const onMatchCount = vi.fn();

    const { container } = render(
      <MarkdownRenderer
        markdown="Alpha beta ALPHA"
        findPattern="alpha"
        findMatchIndex={1}
        onMatchCount={onMatchCount}
      />,
    );

    await waitFor(() => {
      expect(container.querySelectorAll('mark.note-find-hit')).toHaveLength(2);
    });
    expect(container.querySelector('mark.active')?.textContent).toBe('ALPHA');
    expect(onMatchCount).toHaveBeenLastCalledWith(2);
  });

  it('treats regex characters as literal text', async () => {
    const { container } = render(
      <MarkdownRenderer markdown="a.b axb" findPattern="a.b" findMatchIndex={0} />,
    );

    await waitFor(() => {
      expect(container.querySelectorAll('mark.note-find-hit')).toHaveLength(1);
    });
    expect(container.querySelector('mark')?.textContent).toBe('a.b');
  });
});
