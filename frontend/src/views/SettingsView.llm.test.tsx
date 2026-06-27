import { beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { LlmDefaultsPanel } from './SettingsView';
import type { BookConfig, LlmConfig } from '../types';

const mocks = vi.hoisted(() => ({
  listCloudLlmProviderModels: vi.fn(async () => [{ id: 'gpt-5.4-mini' }]),
  updateLlmConfig: vi.fn(async (llm: LlmConfig) => ({ llm })),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

vi.mock('../lib/api', () => ({
  api: {
    listCloudLlmProviderModels: mocks.listCloudLlmProviderModels,
    updateLlmConfig: mocks.updateLlmConfig,
  },
}));

const localRef = { provider: 'local', model: 'gemma-4-e2b' };

function config(): BookConfig {
  return {
    markdown: { dialect_version: '1' },
    summary: { max_chars: 400, max_fraction_of_body: 0.2 },
    cleanup: { default_vanish_days: 30, deletion_delay_days: 30, todo_archive_days: 30 },
    privacy: { unlock_delay_hours: 24, confirmation_delay_hours: 24 },
    embedding: {
      chunk_token_limit: 512,
      chunk_overlap_tokens: 64,
      dimensions: 768,
      model_id: 'embeddinggemma-300m',
      matryoshka_dims: null,
    },
    search: {
      rrf_k: 60,
      category_upweight: 1,
      bm25_k1: 1.2,
      bm25_b: 0.75,
      result_limit: 20,
      related_limit: 8,
      duplicate_similarity: 0.95,
      blind_spot_similarity: 0.35,
    },
    llm: {
      enabled: true,
      provider: 'local',
      local_model: 'gemma-4-e2b',
      max_new_tokens: 512,
      auto_accept: false,
      routing: {
        summarize: localRef,
        fact_check: localRef,
        devils_advocate: localRef,
        grammar: localRef,
        category_suggest: localRef,
        rewrite: localRef,
      },
    },
    sync: {
      enabled: false,
      crdt_backend: 'loro',
      conflict_marker: 'conflict',
      external_edit_skew_secs: 2,
    },
  };
}

function renderPanel() {
  render(
    <LlmDefaultsPanel
      config={config()}
      providers={[
        {
          provider: 'openai_compatible',
          display_name: 'OpenAI-compatible',
          base_url_required: true,
        },
      ]}
      onSaved={vi.fn()}
      onError={vi.fn()}
    />,
  );
}

describe('LlmDefaultsPanel', () => {
  beforeEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it('saves a selected cloud model to every route', async () => {
    renderPanel();

    fireEvent.change(screen.getAllByRole('combobox')[0], {
      target: { value: 'openai_compatible' },
    });
    await waitFor(() => expect(mocks.listCloudLlmProviderModels).toHaveBeenCalledWith('openai_compatible'));
    await screen.findByText('gpt-5.4-mini');
    fireEvent.change(screen.getAllByRole('combobox')[1], {
      target: { value: 'gpt-5.4-mini' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));

    await waitFor(() => expect(mocks.updateLlmConfig).toHaveBeenCalledTimes(1));
    const saved = mocks.updateLlmConfig.mock.calls[0][0];
    expect(saved.provider).toBe('openai_compatible');
    expect(Object.values(saved.routing)).toEqual(
      Array(6).fill({ provider: 'openai_compatible', model: 'gpt-5.4-mini' }),
    );
  });

  it('saves a custom cloud model to every route', async () => {
    renderPanel();

    fireEvent.change(screen.getAllByRole('combobox')[0], {
      target: { value: 'openai_compatible' },
    });
    await waitFor(() => expect(screen.getAllByRole('combobox').length).toBeGreaterThan(1));
    await screen.findByText('Custom model...');
    fireEvent.change(screen.getAllByRole('combobox')[1], {
      target: { value: '__custom_model__' },
    });
    fireEvent.change(screen.getByPlaceholderText(/model name/i), {
      target: { value: 'gpt-5.4-lab' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^save$/i }));

    await waitFor(() => expect(mocks.updateLlmConfig).toHaveBeenCalledTimes(1));
    const saved = mocks.updateLlmConfig.mock.calls[0][0];
    expect(Object.values(saved.routing)).toEqual(
      Array(6).fill({ provider: 'openai_compatible', model: 'gpt-5.4-lab' }),
    );
  });
});
