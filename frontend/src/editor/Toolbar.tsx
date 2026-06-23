// Formatting toolbar for the Lexical body editor: inline marks, block types, and lists.
// Lives inside <LexicalComposer> so it can read the editor via the composer context.

import { useCallback, useState } from 'react';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext';
import {
  $getSelection, $isRangeSelection, $createParagraphNode, FORMAT_TEXT_COMMAND,
} from 'lexical';
import { $setBlocksType } from '@lexical/selection';
import { $createHeadingNode, $createQuoteNode } from '@lexical/rich-text';
import {
  INSERT_UNORDERED_LIST_COMMAND, INSERT_ORDERED_LIST_COMMAND, REMOVE_LIST_COMMAND,
} from '@lexical/list';
import { api } from '../lib/api';
import { Icon } from '../components/Icon';

const IMAGE_FILTER = [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg'] }];

// Syntax sugar snippets inserted at the cursor position.
const SYNTAX_SNIPPETS: { label: string; description: string; text: string }[] = [
  { label: 'Cloze', description: 'Cloze deletion for flashcards', text: '{{cloze text}}' },
  { label: 'due:', description: 'Task due date', text: 'due:' },
  { label: 'start:', description: 'Task start date', text: 'start:' },
  { label: 'done:', description: 'Task completion date', text: 'done:' },
  { label: 'waiting:', description: 'Waiting on another task', text: 'waiting:' },
  { label: 'blocked-by:', description: 'Blocked by another task', text: 'blocked-by:' },
  { label: 'loc:', description: 'Location pin', text: 'loc:' },
  { label: '%%comment%%', description: 'Hidden comment (stripped from exports)', text: '%%comment%%' },
  { label: '#tag', description: 'Inline category tag', text: '#' },
];

export function Toolbar() {
  const [editor] = useLexicalComposerContext();
  const [syntaxOpen, setSyntaxOpen] = useState(false);

  // Copy a chosen image into the book and insert a markdown image reference at the cursor.
  const insertImage = useCallback(async () => {
    const selected = await openDialog({ multiple: false, title: 'Insert image', filters: IMAGE_FILTER });
    if (!selected || typeof selected !== 'string') return;
    const rel = await api.importAsset(selected);
    editor.update(() => {
      const selection = $getSelection();
      if ($isRangeSelection(selection)) selection.insertText(`![image](${rel})`);
    });
  }, [editor]);

  const formatText = useCallback((format: 'bold' | 'italic' | 'strikethrough' | 'code' | 'underline') => {
    editor.dispatchCommand(FORMAT_TEXT_COMMAND, format);
  }, [editor]);

  const setBlock = useCallback((to: 'paragraph' | 'h2' | 'h3' | 'quote') => {
    editor.update(() => {
      const selection = $getSelection();
      if (!$isRangeSelection(selection)) return;
      $setBlocksType(selection, () => {
        if (to === 'h2') return $createHeadingNode('h2');
        if (to === 'h3') return $createHeadingNode('h3');
        if (to === 'quote') return $createQuoteNode();
        return $createParagraphNode();
      });
    });
  }, [editor]);

  const insertSnippet = useCallback((text: string) => {
    editor.update(() => {
      const selection = $getSelection();
      if ($isRangeSelection(selection)) selection.insertText(text);
    });
    setSyntaxOpen(false);
  }, [editor]);

  return (
    <div className="editor-format-toolbar">
      {/* Inline marks */}
      <button title="Bold (⌘B)" onMouseDown={(e) => e.preventDefault()} onClick={() => formatText('bold')}>
        <Icon name="format_bold" size={18} />
      </button>
      <button title="Italic (⌘I)" onMouseDown={(e) => e.preventDefault()} onClick={() => formatText('italic')}>
        <Icon name="format_italic" size={18} />
      </button>
      <button title="Underline (⌘U)" onMouseDown={(e) => e.preventDefault()} onClick={() => formatText('underline')}>
        <Icon name="format_underlined" size={18} />
      </button>
      <button title="Strikethrough" onMouseDown={(e) => e.preventDefault()} onClick={() => formatText('strikethrough')}>
        <Icon name="strikethrough_s" size={18} />
      </button>
      <button title="Inline code" onMouseDown={(e) => e.preventDefault()} onClick={() => formatText('code')}>
        <Icon name="code" size={18} />
      </button>

      <span className="editor-format-divider" />

      {/* Block types */}
      <button title="Normal text" onMouseDown={(e) => e.preventDefault()} onClick={() => setBlock('paragraph')}>
        <Icon name="notes" size={18} />
      </button>
      <button title="Heading 2" onMouseDown={(e) => e.preventDefault()} onClick={() => setBlock('h2')}>
        <Icon name="title" size={18} />
      </button>
      <button title="Heading 3" onMouseDown={(e) => e.preventDefault()} onClick={() => setBlock('h3')}>
        <span style={{ fontSize: 13, fontWeight: 600 }}>H3</span>
      </button>
      <button title="Block quote" onMouseDown={(e) => e.preventDefault()} onClick={() => setBlock('quote')}>
        <Icon name="format_quote" size={18} />
      </button>

      <span className="editor-format-divider" />

      {/* Lists */}
      <button
        title="Bullet list"
        onMouseDown={(e) => e.preventDefault()}
        onClick={() => editor.dispatchCommand(INSERT_UNORDERED_LIST_COMMAND, undefined)}
      >
        <Icon name="format_list_bulleted" size={18} />
      </button>
      <button
        title="Numbered list"
        onMouseDown={(e) => e.preventDefault()}
        onClick={() => editor.dispatchCommand(INSERT_ORDERED_LIST_COMMAND, undefined)}
      >
        <Icon name="format_list_numbered" size={18} />
      </button>
      <button
        title="Remove list"
        onMouseDown={(e) => e.preventDefault()}
        onClick={() => editor.dispatchCommand(REMOVE_LIST_COMMAND, undefined)}
      >
        <Icon name="format_clear" size={18} />
      </button>

      <span className="editor-format-divider" />

      {/* Insert */}
      <button title="Insert image" onMouseDown={(e) => e.preventDefault()} onClick={insertImage}>
        <Icon name="image" size={18} />
      </button>

      {/* Syntax sugar dropdown */}
      <div className="editor-syntax-dropdown">
        <button
          title="Insert syntax token"
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => setSyntaxOpen((v) => !v)}
          className={syntaxOpen ? 'active' : ''}
        >
          <Icon name="add_circle_outline" size={18} />
          <span style={{ fontSize: 11, marginLeft: 2 }}>Insert</span>
        </button>
        {syntaxOpen && (
          <div className="editor-syntax-menu">
            {SYNTAX_SNIPPETS.map((s) => (
              <button
                key={s.label}
                className="editor-syntax-item"
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => insertSnippet(s.text)}
                title={s.description}
              >
                <code className="editor-syntax-item-code">{s.label}</code>
                <span className="editor-syntax-item-desc">{s.description}</span>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
