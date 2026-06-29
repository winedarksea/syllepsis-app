/* eslint-disable react-refresh/only-export-components -- Lexical node file: mixes a class, helper fns, and an internal React component by design */
// A fenced code block whose language is claimed by a code-block-renderer plugin. Instead of plain
// `<pre><code>`, it asks the backend plugin to render the block and shows the (sanitized) HTML.
// Round-trips back to a ```lang fenced block on export (see createPluginCodeTransformer).

import { DecoratorBlockNode } from '@lexical/react/LexicalDecoratorBlockNode';
import type { SerializedDecoratorBlockNode } from '@lexical/react/LexicalDecoratorBlockNode';
import type { ElementFormatType, LexicalNode, NodeKey, Spread } from 'lexical';
import { useEffect, useState } from 'react';
import { api } from '../../lib/api';
import { sanitizeHtml } from '../../lib/sanitize';

export type SerializedPluginBlockNode = Spread<
  { language: string; code: string },
  SerializedDecoratorBlockNode
>;

function PluginBlock({ language, code }: { language: string; code: string }) {
  const [html, setHtml] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let active = true;
    setHtml(null);
    setFailed(false);
    api
      .runRenderPlugin(language, code)
      .then((raw) => { if (active) setHtml(sanitizeHtml(raw)); })
      .catch(() => { if (active) setFailed(true); });
    return () => { active = false; };
  }, [language, code]);

  if (html !== null && !failed) {
    return (
      <div
        className="plugin-block"
        data-language={language}
        contentEditable={false}
        dangerouslySetInnerHTML={{ __html: html }}
      />
    );
  }
  // Loading or error: show the raw source so content is never hidden.
  return (
    <pre className="plugin-block plugin-block-fallback" data-language={language} contentEditable={false}>
      <code>{code}</code>
    </pre>
  );
}

export class PluginBlockNode extends DecoratorBlockNode {
  __language: string;
  __code: string;

  static getType() { return 'plugin-block'; }

  static clone(node: PluginBlockNode): PluginBlockNode {
    return new PluginBlockNode(node.__language, node.__code, node.__format, node.__key);
  }

  static importJSON(data: SerializedPluginBlockNode): PluginBlockNode {
    return new PluginBlockNode(data.language, data.code, data.format);
  }

  constructor(language: string, code: string, format?: ElementFormatType, key?: NodeKey) {
    super(format, key);
    this.__language = language;
    this.__code = code;
  }

  exportJSON(): SerializedPluginBlockNode {
    return {
      ...super.exportJSON(),
      type: 'plugin-block',
      version: 1,
      language: this.__language,
      code: this.__code,
    };
  }

  getLanguage(): string { return this.__language; }
  getCode(): string { return this.__code; }

  getTextContent(): string { return this.__code; }

  decorate() {
    return <PluginBlock language={this.__language} code={this.__code} />;
  }
}

export function $createPluginBlockNode(language: string, code: string): PluginBlockNode {
  return new PluginBlockNode(language, code);
}

export function $isPluginBlockNode(
  node: LexicalNode | null | undefined,
): node is PluginBlockNode {
  return node instanceof PluginBlockNode;
}
