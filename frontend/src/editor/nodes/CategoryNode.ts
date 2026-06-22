// Renders #category inline tags as styled chips. Lexical custom node.

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

export type SerializedCategoryNode = Spread<
  { name: string },
  SerializedLexicalNode
>;

function convertCategoryElement(el: HTMLElement): DOMConversionOutput | null {
  const name = el.dataset.category;
  if (!name) return null;
  return { node: $createCategoryNode(name) };
}

export class CategoryNode extends DecoratorNode<React.ReactElement> {
  __name: string;

  static getType() { return 'category'; }

  static clone(node: CategoryNode) {
    return new CategoryNode(node.__name, node.__key);
  }

  static importJSON(data: SerializedCategoryNode): CategoryNode {
    return $createCategoryNode(data.name);
  }

  static importDOM(): DOMConversionMap {
    return {
      span: (el) =>
        (el as HTMLElement).dataset.category
          ? { conversion: convertCategoryElement, priority: 1 }
          : null,
    };
  }

  constructor(name: string, key?: NodeKey) {
    super(key);
    this.__name = name;
  }

  exportJSON(): SerializedCategoryNode {
    return { ...super.exportJSON(), type: 'category', name: this.__name };
  }

  exportDOM(): DOMExportOutput {
    const el = document.createElement('span');
    el.dataset.category = this.__name;
    el.textContent = `#${this.__name}`;
    return { element: el };
  }

  createDOM() {
    const span = document.createElement('span');
    span.className = 'lexical-category-node';
    return span;
  }

  updateDOM() { return false; }

  decorate() {
    return createElement('span', { className: 'lexical-category-chip' }, `#${this.__name}`);
  }

  isInline() { return true; }
  isIsolated() { return true; }
  isKeyboardSelectable() { return true; }
}

export function $createCategoryNode(name: string): CategoryNode {
  return $applyNodeReplacement(new CategoryNode(name));
}

export function $isCategoryNode(node: LexicalNode | null | undefined): node is CategoryNode {
  return node instanceof CategoryNode;
}
