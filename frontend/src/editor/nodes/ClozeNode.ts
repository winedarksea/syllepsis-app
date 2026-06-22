// Renders ||cloze|hint|| deletion gaps. Lexical custom inline node.

import {
  $applyNodeReplacement,
  DecoratorNode,
} from 'lexical';
import type {
  DOMConversionMap,
  DOMConversionOutput,
  DOMExportOutput,
  LexicalNode,
  NodeKey,
  SerializedLexicalNode,
  Spread,
} from 'lexical';
import { createElement } from 'react';

export type SerializedClozeNode = Spread<
  { text: string; hint: string },
  SerializedLexicalNode
>;

function convertClozeElement(el: HTMLElement): DOMConversionOutput | null {
  const text = el.dataset.clozeText ?? '';
  const hint = el.dataset.clozeHint ?? '';
  return { node: $createClozeNode(text, hint) };
}

export class ClozeNode extends DecoratorNode<React.ReactElement> {
  __text: string;
  __hint: string;

  static getType() { return 'cloze'; }

  static clone(node: ClozeNode) {
    return new ClozeNode(node.__text, node.__hint, node.__key);
  }

  static importJSON(data: SerializedClozeNode): ClozeNode {
    return $createClozeNode(data.text, data.hint);
  }

  static importDOM(): DOMConversionMap {
    return {
      span: (el) =>
        (el as HTMLElement).dataset.clozeText !== undefined
          ? { conversion: convertClozeElement, priority: 1 }
          : null,
    };
  }

  constructor(text: string, hint: string, key?: NodeKey) {
    super(key);
    this.__text = text;
    this.__hint = hint;
  }

  exportJSON(): SerializedClozeNode {
    return { ...super.exportJSON(), type: 'cloze', text: this.__text, hint: this.__hint };
  }

  exportDOM(): DOMExportOutput {
    const el = document.createElement('span');
    el.dataset.clozeText = this.__text;
    el.dataset.clozeHint = this.__hint;
    el.textContent = `||${this.__text}|${this.__hint}||`;
    return { element: el };
  }

  createDOM() {
    const span = document.createElement('span');
    span.className = 'lexical-cloze-node';
    return span;
  }

  updateDOM() { return false; }

  decorate() {
    const label = this.__hint ? `${this.__text} (${this.__hint})` : this.__text;
    return createElement('span', { className: 'lexical-cloze-chip', title: `Cloze: ${label}` }, `[${this.__text}]`);
  }

  isInline() { return true; }
  isIsolated() { return true; }
}

export function $createClozeNode(text: string, hint = ''): ClozeNode {
  return $applyNodeReplacement(new ClozeNode(text, hint));
}

export function $isClozeNode(node: LexicalNode | null | undefined): node is ClozeNode {
  return node instanceof ClozeNode;
}
