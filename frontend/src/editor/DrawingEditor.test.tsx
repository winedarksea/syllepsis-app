import { render, waitFor } from '@testing-library/react';
import type { MutableRefObject } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DrawingEditor } from './DrawingEditor';
import type { NoteDto } from '../types';

const mocks = vi.hoisted(() => ({
  readDrawingSvg: vi.fn(async () => ''),
  listNotes: vi.fn(async () => [] as NoteDto[]),
  updateNote: vi.fn(async (note: NoteDto) => note),
}));

vi.mock('@excalidraw/excalidraw', () => ({
  Excalidraw: () => <div data-testid="excalidraw" />,
  exportToSvg: vi.fn(async () => document.createElementNS('http://www.w3.org/2000/svg', 'svg')),
  serializeAsJSON: vi.fn(() => '{}'),
  hashElementsVersion: vi.fn(() => 1),
  convertToExcalidrawElements: vi.fn(() => []),
}));

vi.mock('../lib/api', () => ({
  api: {
    readDrawingSvg: mocks.readDrawingSvg,
    listNotes: mocks.listNotes,
    updateNote: mocks.updateNote,
    assetData: vi.fn(async () => null),
  },
}));

function drawingNote(overrides: Partial<NoteDto> = {}): NoteDto {
  return {
    id: 'drawing-01j00000000000000000000000',
    type: 'drawing',
    title: 'Drawing',
    summary: '',
    body: '',
    categories: [],
    sorted: false,
    asset: {
      uuid: 'asset-1',
      media_type: 'image/svg+xml',
      intrinsic_dimensions: [100, 100],
      original_filename: 'drawing.svg',
    },
    metadata: {
      classification: {
        kind: 'note',
        basis: 'none',
        checkability: 'none',
        stability: 'settled',
        priority: 'standard',
        starred: false,
        stylistic_elements: [],
      },
      dates: {
        created: '2026-07-07T00:00:00Z',
        updated: '2026-07-07T00:00:00Z',
      },
      authorship: {},
      packs: {},
      kanban: {},
    },
    ...overrides,
  };
}

describe('DrawingEditor save link sync', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    const scene = {
      type: 'excalidraw',
      elements: [{ id: 'el-1', link: 'syllepsis://note/note-2' }],
      appState: {},
      files: {},
    };
    mocks.readDrawingSvg.mockResolvedValue(
      `<svg xmlns="http://www.w3.org/2000/svg"><metadata>${JSON.stringify(scene)}</metadata></svg>`,
    );
    mocks.listNotes.mockResolvedValue([
      drawingNote({ id: 'note-2', type: 'note', title: 'Linked target', asset: undefined }),
    ]);
    mocks.updateNote.mockImplementation(async (note: NoteDto) => note);
  });

  it('syncs linked-note body from the latest note and sends a baseline body', async () => {
    const getSvgRef = { current: null } as MutableRefObject<(() => Promise<string | null>) | null>;
    render(
      <DrawingEditor
        note={drawingNote({ body: 'stale body' })}
        markDirty={() => {}}
        getSvgRef={getSvgRef}
      />,
    );
    await waitFor(() => expect(mocks.readDrawingSvg).toHaveBeenCalled());
    await waitFor(() => expect(mocks.listNotes).toHaveBeenCalled());

    const latest = drawingNote({
      body: 'fresh body',
      asset: {
        uuid: 'asset-1',
        media_type: 'image/svg+xml',
        intrinsic_dimensions: [300, 200],
        original_filename: 'drawing.svg',
      },
    });
    const updated = await (getSvgRef as unknown as {
      _syncLinks: (noteForSync: NoteDto) => Promise<NoteDto>;
    })._syncLinks(latest);

    expect(updated.body).toContain('fresh body');
    expect(updated.body).toContain('[Linked target](syllepsis://note/note-2)');
    expect(mocks.updateNote).toHaveBeenCalledWith({
      ...latest,
      body: updated.body,
      baseline_body: 'fresh body',
    });
  });
});
