// Lexical-based note editor. Handles a single NoteDto; saves on Cmd+S or the Save button.

import { useCallback, useEffect, useRef, useState } from 'react';
import { LexicalComposer, type InitialConfigType } from '@lexical/react/LexicalComposer';
import { RichTextPlugin } from '@lexical/react/LexicalRichTextPlugin';
import { ContentEditable } from '@lexical/react/LexicalContentEditable';
import { HistoryPlugin } from '@lexical/react/LexicalHistoryPlugin';
import { OnChangePlugin } from '@lexical/react/LexicalOnChangePlugin';
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext';
import { $getRoot, $createParagraphNode, $createTextNode } from 'lexical';
import type { EditorState } from 'lexical';
import { CategoryNode } from './nodes/CategoryNode';
import { ClozeNode } from './nodes/ClozeNode';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import { Icon } from '../components/Icon';
import type { NoteDto } from '../types';
import { RelatedCarousel } from '../components/RelatedCarousel';
import './Editor.css';

// Plugin: initialises the editor with the note's body text on mount.
function InitBodyPlugin({ body }: { body: string }) {
  const [editor] = useLexicalComposerContext();
  const initialised = useRef(false);

  useEffect(() => {
    if (initialised.current) return;
    initialised.current = true;
    editor.update(() => {
      const root = $getRoot();
      root.clear();
      const para = $createParagraphNode();
      para.append($createTextNode(body));
      root.append(para);
    });
  }, [editor, body]);

  return null;
}

// Plugin: listen for Cmd+S / Ctrl+S to save.
function SaveShortcutPlugin({ onSave }: { onSave: () => void }) {
  const [editor] = useLexicalComposerContext();

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 's') {
        e.preventDefault();
        onSave();
      }
    };
    editor.getRootElement()?.addEventListener('keydown', handler);
    return () => editor.getRootElement()?.removeEventListener('keydown', handler);
  }, [editor, onSave]);

  return null;
}

interface Props {
  noteId: string;
}

export function Editor({ noteId }: Props) {
  const { closeEditor } = useStore();
  const [note, setNote] = useState<NoteDto | null>(null);
  const [title, setTitle] = useState('');
  const [summary, setSummary] = useState('');
  const [body, setBody] = useState('');
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.getNote(noteId).then((n) => {
      setNote(n);
      setTitle(n.title);
      setSummary(n.summary);
      setBody(n.body);
    }).catch((e) => setError(String(e)));
  }, [noteId]);

  const getCurrentBody = useRef<() => string>(() => body);

  const handleEditorChange = useCallback((state: EditorState) => {
    state.read(() => {
      const text = $getRoot().getTextContent();
      getCurrentBody.current = () => text;
      setDirty(true);
    });
  }, []);

  const save = useCallback(async () => {
    if (!note) return;
    setSaving(true);
    setError(null);
    try {
      const updated = await api.updateNote({
        ...note,
        title,
        summary,
        body: getCurrentBody.current(),
      });
      setNote(updated);
      setDirty(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }, [note, title, summary]);

  if (!note) {
    return (
      <div className="editor-loading">
        {error ? <span className="editor-error">{error}</span> : 'Loading…'}
      </div>
    );
  }

  const editorConfig: InitialConfigType = {
    namespace: `note-${noteId}`,
    nodes: [CategoryNode, ClozeNode],
    onError: (err) => setError(err.message),
    theme: {
      root: 'lexical-root',
      paragraph: 'lexical-paragraph',
      text: {
        bold: 'lexical-bold',
        italic: 'lexical-italic',
        underline: 'lexical-underline',
        code: 'lexical-code-inline',
      },
    },
  };

  return (
    <div className="editor-container selectable">
      <div className="editor-toolbar">
        <button className="editor-back" onClick={closeEditor}>
          <Icon name="arrow_back" size={16} />
          <span>Back</span>
        </button>
        <div className="editor-toolbar-center">
          <span className="editor-type-badge">{note.type}</span>
        </div>
        <div className="editor-toolbar-actions">
          {dirty && <span className="editor-dirty-dot" title="Unsaved changes" />}
          <button
            className="editor-save-btn"
            onClick={save}
            disabled={saving || !dirty}
          >
            {saving ? 'Saving…' : 'Save'}
          </button>
        </div>
      </div>

      {error && <div className="editor-error-banner">{error}</div>}

      <div className="editor-meta">
        <input
          className="editor-title"
          value={title}
          onChange={(e) => { setTitle(e.target.value); setDirty(true); }}
          placeholder="Note title…"
        />
        <input
          className="editor-summary"
          value={summary}
          onChange={(e) => { setSummary(e.target.value); setDirty(true); }}
          placeholder="One-line summary (optional)…"
        />
        <div className="editor-categories">
          {note.categories.map((c) => (
            <span key={c} className="editor-category-chip">#{c}</span>
          ))}
        </div>
      </div>

      <LexicalComposer initialConfig={editorConfig}>
        <RichTextPlugin
          contentEditable={<ContentEditable className="lexical-content-editable" />}
          placeholder={<div className="lexical-placeholder">Start writing…</div>}
          ErrorBoundary={({ onError, children }) => {
            try { return <>{children}</>; }
            catch (e) { onError(e as Error); return null; }
          }}
        />
        <HistoryPlugin />
        <OnChangePlugin onChange={handleEditorChange} />
        <InitBodyPlugin body={body} />
        <SaveShortcutPlugin onSave={save} />
      </LexicalComposer>

      <RelatedCarousel noteId={noteId} />
    </div>
  );
}
