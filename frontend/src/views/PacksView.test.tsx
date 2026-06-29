import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PacksView } from './PacksView';
import { useStore } from '../lib/store';
import type { ImportPreview, ImportReport, NoteResolution } from '../types';

const dialogMocks = vi.hoisted(() => ({
  openDialog: vi.fn(),
  saveDialog: vi.fn(),
}));

const apiMocks = vi.hoisted(() => ({
  previewPack: vi.fn(),
  importPack: vi.fn(),
  exportPack: vi.fn(),
  importPackAsBook: vi.fn(),
  allCategories: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: dialogMocks.openDialog,
  save: dialogMocks.saveDialog,
}));

vi.mock('../lib/api', () => ({
  api: {
    previewPack: apiMocks.previewPack,
    importPack: apiMocks.importPack,
    exportPack: apiMocks.exportPack,
    importPackAsBook: apiMocks.importPackAsBook,
    allCategories: apiMocks.allCategories,
  },
}));

vi.mock('../components/Icon', () => ({
  Icon: ({ name }: { name: string }) => <span aria-hidden="true">{name}</span>,
}));

function importReport(): ImportReport {
  return {
    imported: [],
    skipped_locally_modified: [],
    created_categories: [],
    overwritten: [],
    merged: [],
    commentary_created: [],
    duplicated: [],
  };
}

function preview(notes: ImportPreview['notes']): ImportPreview {
  return {
    manifest: {
      id: 'garden-pack',
      name: 'Garden Pack',
      version: '1.1.0',
      description: '',
      export_kind: 'pack',
    },
    notes,
    category_suggestions: [],
  };
}

function renderPacksView() {
  useStore.setState({
    book: { name: 'Test Book', path: '/books/test' } as any,
    categories: [{ name: 'garden' } as any],
  });
  render(<PacksView />);
}

async function choosePack(path = '/packs/garden.synpack.json') {
  dialogMocks.openDialog.mockResolvedValueOnce(path);
  fireEvent.click(screen.getByRole('button', { name: 'Import' }));
  fireEvent.click(screen.getByRole('button', { name: /choose pack file/i }));
  await screen.findByText(/Garden Pack/);
}

function resolutionSelect(): HTMLSelectElement {
  const select = document.querySelector('.pk-resolution-select');
  if (!(select instanceof HTMLSelectElement)) {
    throw new Error('expected a resolution select');
  }
  return select;
}

describe('PacksView', () => {
  beforeEach(() => {
    cleanup();
    vi.clearAllMocks();
    apiMocks.importPack.mockResolvedValue(importReport());
    apiMocks.exportPack.mockResolvedValue({
      id: 'garden-pack',
      name: 'Garden Pack',
      version: '1.0.0',
      description: '',
      export_kind: 'pack',
    });
    apiMocks.previewPack.mockResolvedValue(
      preview([
        { id: 'note-new', title: 'New Note', status: 'new' },
        { id: 'note-update', title: 'Update Note', status: 'update' },
        { id: 'note-local', title: 'Local Note', status: 'locally_modified' },
      ]),
    );
    dialogMocks.saveDialog.mockResolvedValue('/packs/export.synpack.json');
  });

  it('shows resolution select only for selected locally modified notes', async () => {
    renderPacksView();
    await choosePack();

    expect(document.querySelectorAll('.pk-resolution-select')).toHaveLength(1);

    const localRow = screen.getByText('Local Note').closest('.pk-note-row');
    const checkbox = localRow?.querySelector('input[type="checkbox"]');
    if (!(checkbox instanceof HTMLInputElement)) {
      throw new Error('expected local note checkbox');
    }
    fireEvent.click(checkbox);

    expect(document.querySelectorAll('.pk-resolution-select')).toHaveLength(0);
  });

  it('defaults locally modified resolution to skip', async () => {
    renderPacksView();
    await choosePack();

    fireEvent.click(screen.getByRole('button', { name: /import 3 notes/i }));

    await waitFor(() => expect(apiMocks.importPack).toHaveBeenCalledTimes(1));
    expect(apiMocks.importPack).toHaveBeenCalledWith('/packs/garden.synpack.json', {
      selected_note_ids: ['note-new', 'note-update', 'note-local'],
      category_map: {},
      resolutions: {},
    });
  });

  it('serializes commentary overwrite merge duplicate resolution', async () => {
    for (const resolution of ['commentary', 'overwrite', 'merge', 'duplicate'] satisfies NoteResolution[]) {
      cleanup();
      vi.clearAllMocks();
      apiMocks.importPack.mockResolvedValue(importReport());
      apiMocks.previewPack.mockResolvedValue(
        preview([{ id: 'note-local', title: 'Local Note', status: 'locally_modified' }]),
      );
      renderPacksView();
      await choosePack(`/packs/${resolution}.synpack.json`);

      fireEvent.change(resolutionSelect(), { target: { value: resolution } });
      fireEvent.click(screen.getByRole('button', { name: /import 1 note/i }));

      await waitFor(() => expect(apiMocks.importPack).toHaveBeenCalledTimes(1));
      expect(apiMocks.importPack).toHaveBeenCalledWith(`/packs/${resolution}.synpack.json`, {
        selected_note_ids: ['note-local'],
        category_map: {},
        resolutions: { 'note-local': resolution },
      });
    }
  });

  it('resets resolutions when a new pack is chosen', async () => {
    apiMocks.previewPack
      .mockResolvedValueOnce(
        preview([{ id: 'note-a', title: 'Pack A Local', status: 'locally_modified' }]),
      )
      .mockResolvedValueOnce(
        preview([{ id: 'note-b', title: 'Pack B Local', status: 'locally_modified' }]),
      );
    renderPacksView();

    await choosePack('/packs/a.synpack.json');
    fireEvent.change(resolutionSelect(), { target: { value: 'commentary' } });
    await choosePack('/packs/b.synpack.json');
    fireEvent.click(screen.getByRole('button', { name: /import 1 note/i }));

    await waitFor(() => expect(apiMocks.importPack).toHaveBeenCalledTimes(1));
    expect(apiMocks.importPack).toHaveBeenCalledWith('/packs/b.synpack.json', {
      selected_note_ids: ['note-b'],
      category_map: {},
      resolutions: {},
    });
  });

  it('export includes commentary flag', async () => {
    renderPacksView();

    fireEvent.change(screen.getByPlaceholderText('permaculture-basics'), {
      target: { value: 'garden-pack' },
    });
    fireEvent.click(screen.getByLabelText(/include commentary/i));
    fireEvent.click(screen.getByLabelText('#garden'));
    fireEvent.click(screen.getByRole('button', { name: /export pack/i }));

    await waitFor(() => expect(apiMocks.exportPack).toHaveBeenCalledTimes(1));
    expect(apiMocks.exportPack).toHaveBeenCalledWith(
      {
        id: 'garden-pack',
        name: 'Test Book',
        version: '1.0.0',
        description: '',
        categories: ['garden'],
        note_ids: [],
        export_all: false,
        include_commentary: true,
      },
      '/packs/export.synpack.json',
    );
  });
});
