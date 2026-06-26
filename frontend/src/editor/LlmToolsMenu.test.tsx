import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { LlmToolsMenu } from './LlmToolsMenu';

const mocks = vi.hoisted(() => ({
  enqueueLlmJob: vi.fn(async () => ({
    job_id: 'job-1',
    status: 'queued',
    target_note_id: 'note-1',
    task: 'rewrite',
    proposal: null,
    error: null,
  })),
}));

vi.mock('../lib/api', () => ({
  api: {
    llmRouteStatuses: vi.fn(async () => [
      {
        task: 'rewrite',
        provider: 'local',
        model: 'gemma',
        execution_mode: 'local',
        available: true,
      },
      {
        task: 'summarize',
        provider: 'local',
        model: 'gemma',
        execution_mode: 'local',
        available: true,
      },
    ]),
    cloudLlmProviderDescriptors: vi.fn(async () => []),
    listStyleCards: vi.fn(async () => [
      {
        id: 'style-1',
        version: 1,
        name: 'Plainspoken',
        short_description: 'Direct prose',
        verbosity: 'succinct',
        perspective: 'second_person',
        reading_level: 'accessible',
        voice: 'active',
        patterns: [],
        exemplars: [],
        source_urls: [],
      },
    ]),
    enqueueLlmJob: mocks.enqueueLlmJob,
  },
}));

vi.mock('../components/Icon', () => ({
  Icon: ({ name }: { name: string }) => <span aria-hidden="true">{name}</span>,
}));

describe('LlmToolsMenu', () => {
  it('serializes rewrite options into a queued job request', async () => {
    render(<LlmToolsMenu noteId="note-1" />);

    fireEvent.click(screen.getByRole('button', { name: /tools/i }));
    const toolSelect = await screen.findByLabelText(/tool/i);
    fireEvent.change(toolSelect, { target: { value: 'rewrite' } });

    fireEvent.change(screen.getByLabelText(/^Mode$/), { target: { value: 'simplify' } });
    fireEvent.change(screen.getByLabelText(/style card/i), { target: { value: 'style-1' } });
    fireEvent.change(screen.getByPlaceholderText(/additional one-run style notes/i), {
      target: { value: 'Use shorter paragraphs.' },
    });

    fireEvent.click(screen.getByRole('button', { name: /queue job/i }));

    await waitFor(() => expect(mocks.enqueueLlmJob).toHaveBeenCalledTimes(1));
    expect(mocks.enqueueLlmJob).toHaveBeenCalledWith({
      target_note_id: 'note-1',
      task: 'rewrite',
      model_override: null,
      style_card_id: 'style-1',
      style_overrides: [
        'verbosity: succinct',
        'perspective: second_person',
        'reading_level: accessible',
        'voice: active',
        'Use shorter paragraphs.',
      ].join('\n'),
      summary_variant: 'plain',
      rewrite_mode: 'simplify',
      store_result_as_commentary: true,
    });
  });
});
