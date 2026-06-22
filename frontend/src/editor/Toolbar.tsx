// Formatting toolbar for the Lexical body editor: inline marks, block types, and lists.
// Lives inside <LexicalComposer> so it can read the editor via the composer context.

import { useCallback } from 'react';
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

export function Toolbar() {
  const [editor] = useLexicalComposerContext();

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

  const formatText = useCallback((format: 'bold' | 'italic') => {
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

  return (
    <div className="editor-format-toolbar">
      <button title="Bold" onMouseDown={(e) => e.preventDefault()} onClick={() => formatText('bold')}>
        <Icon name="format_bold" size={18} />
      </button>
      <button title="Italic" onMouseDown={(e) => e.preventDefault()} onClick={() => formatText('italic')}>
        <Icon name="format_italic" size={18} />
      </button>
      <span className="editor-format-divider" />
      <button title="Normal text" onMouseDown={(e) => e.preventDefault()} onClick={() => setBlock('paragraph')}>
        <Icon name="notes" size={18} />
      </button>
      <button title="Heading" onMouseDown={(e) => e.preventDefault()} onClick={() => setBlock('h2')}>
        <Icon name="title" size={18} />
      </button>
      <button title="Quote" onMouseDown={(e) => e.preventDefault()} onClick={() => setBlock('quote')}>
        <Icon name="format_quote" size={18} />
      </button>
      <span className="editor-format-divider" />
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
      <button title="Insert image" onMouseDown={(e) => e.preventDefault()} onClick={insertImage}>
        <Icon name="image" size={18} />
      </button>
    </div>
  );
}
