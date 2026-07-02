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

  it('highlights a match that spans multiple text nodes across inline markup', async () => {
    // Text nodes after parsing: "al", "pha", " beta alpha" — concatenated: "alpha beta alpha".
    // Two logical matches: the first split across the "al"/"pha" node boundary (<em> inserts a
    // node break mid-word), the second entirely inside the trailing text node.
    mocks.renderNoteMarkdown.mockResolvedValueOnce('<p>al<em>pha</em> beta alpha</p>');
    const onMatchCount = vi.fn();

    const { container } = render(
      <MarkdownRenderer markdown="al*pha* beta alpha" findPattern="alpha" findMatchIndex={0} onMatchCount={onMatchCount} />,
    );

    await waitFor(() => {
      expect(container.querySelectorAll('mark.note-find-hit')).toHaveLength(3);
    });
    const marks = container.querySelectorAll('mark.note-find-hit');
    expect(marks[0].textContent).toBe('al');
    expect(marks[1].textContent).toBe('pha');
    expect(marks[2].textContent).toBe('alpha');
    // findMatchIndex={0} selects the first logical match (split across marks[0] and marks[1]).
    expect(marks[0].classList.contains('active')).toBe(true);
    expect(marks[1].classList.contains('active')).toBe(true);
    expect(marks[2].classList.contains('active')).toBe(false);
    expect(onMatchCount).toHaveBeenLastCalledWith(2);
  });
});
