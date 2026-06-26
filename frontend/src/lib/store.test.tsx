import { beforeEach, describe, expect, it } from 'vitest';
import { useStore } from './store';

describe('editor return navigation', () => {
  beforeEach(() => {
    useStore.setState({
      view: 'unsorted',
      editingNoteId: null,
      editingMode: 'read',
      editorReturnView: null,
    });
  });

  it('returns to graph after opening a note from graph', () => {
    useStore.getState().setView('graph');
    useStore.getState().openEditor('note-1');

    expect(useStore.getState().view).toBe('editor');
    expect(useStore.getState().editingMode).toBe('edit');
    expect(useStore.getState().editorReturnView).toBe('graph');

    useStore.getState().closeEditor();

    expect(useStore.getState().view).toBe('graph');
    expect(useStore.getState().editingNoteId).toBeNull();
    expect(useStore.getState().editorReturnView).toBeNull();
  });

  it('preserves the original return view when moving between notes in the editor', () => {
    useStore.getState().setView('graph');
    useStore.getState().openEditor('note-1');
    useStore.getState().openEditor('note-2');
    useStore.getState().closeEditor();

    expect(useStore.getState().view).toBe('graph');
  });

  it('returns to unsorted when no return view was captured', () => {
    useStore.setState({ view: 'editor', editorReturnView: null, editingNoteId: 'note-1' });
    useStore.getState().closeEditor();

    expect(useStore.getState().view).toBe('unsorted');
  });
});
