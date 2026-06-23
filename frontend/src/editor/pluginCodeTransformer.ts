// A Lexical markdown transformer that maps fenced code blocks whose language is claimed by a
// code-block-renderer plugin to a PluginBlockNode (which renders via the plugin). Its start regex
// only matches registered languages, so all other code fences fall through to the built-in CODE
// transformer untouched. Must be listed *before* the default transformers.

import type { MultilineElementTransformer } from '@lexical/markdown';
import { $createPluginBlockNode, $isPluginBlockNode, PluginBlockNode } from './nodes/PluginBlockNode';

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Build the transformer for the given plugin-claimed languages, or `null` when there are none
 * (so callers can simply spread it in conditionally).
 */
export function createPluginCodeTransformer(
  languages: string[],
): MultilineElementTransformer | null {
  const claimed = new Set(languages.map((l) => l.toLowerCase()));
  if (claimed.size === 0) return null;

  const alternation = [...claimed].map(escapeRegExp).join('|');
  // Mirrors the built-in CODE start regex but restricted to claimed languages: [1]=fence, [2]=lang.
  const regExpStart = new RegExp(`^([ \\t]*\`{3,})(${alternation})[ \\t]?$`, 'i');
  const regExpEnd = /^[ \t]*`{3,}$/;

  return {
    type: 'multiline-element',
    dependencies: [PluginBlockNode],
    export: (node) => {
      if (!$isPluginBlockNode(node)) return null;
      const code = node.getCode();
      return '```' + node.getLanguage() + (code ? '\n' + code : '') + '\n```';
    },
    regExpStart,
    regExpEnd: { optional: true, regExp: regExpEnd },
    replace: (rootNode, children, startMatch, _endMatch, linesInBetween) => {
      const language = (startMatch[2] || '').toLowerCase();
      if (!claimed.has(language)) return false; // not ours — let the default CODE transformer run.
      // Only the import path (linesInBetween present, no children) is handled; live typing falls
      // through to the built-in code behavior.
      if (children || !linesInBetween) return false;

      const lines = [...linesInBetween];
      if (lines.length > 0 && lines[0].startsWith(' ')) lines[0] = lines[0].slice(1);
      while (lines.length > 0 && lines[lines.length - 1].length === 0) lines.pop();

      rootNode.append($createPluginBlockNode(language, lines.join('\n')));
    },
  };
}
