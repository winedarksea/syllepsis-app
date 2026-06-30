import { describe, expect, it } from 'vitest';
import type { GraphAnalysisNode, NoteStatus, Priority } from '../types';
import {
  filterKanbanNodes,
  groupKanbanNodes,
  kanbanCardColorClass,
  kanbanSectionForStatus,
} from './kanbanModel';

describe('kanban model', () => {
  it('groups statuses into the three visible sections', () => {
    expect(kanbanSectionForStatus(undefined)).toBe('todo');
    expect(kanbanSectionForStatus('open')).toBe('todo');
    expect(kanbanSectionForStatus('deferred')).toBe('todo');
    expect(kanbanSectionForStatus('needs_clarification')).toBe('todo');
    expect(kanbanSectionForStatus('active')).toBe('active');
    expect(kanbanSectionForStatus('done')).toBe('done');
    expect(kanbanSectionForStatus('cancelled')).toBe('done');
  });

  it('orders done notes before cancelled notes', () => {
    const grouped = groupKanbanNodes([
      node('cancelled task', 'cancelled', ['work'], 'standard', '2024-01-03T00:00:00Z'),
      node('done task', 'done', ['work'], 'standard', '2024-01-01T00:00:00Z'),
    ]);

    expect(grouped.done.map((entry) => entry.status)).toEqual(['done', 'cancelled']);
  });

  it('filters no-status cards, categories with any selected match, and priorities', () => {
    const nodes = [
      node('none', undefined, ['work'], 'standard'),
      node('mixed category', 'open', ['home', 'work'], 'important'),
      node('other priority', 'open', ['home'], 'core'),
    ];

    const filtered = filterKanbanNodes(nodes, {
      selectedCategories: ['work'],
      selectedPriorities: ['important'],
      showNoStatus: false,
    });

    expect(filtered.map((entry) => entry.title)).toEqual(['mixed category']);
  });

  it('returns stable color classes for each color mode', () => {
    const categoryPalette = new Map([['work', 2]]);
    const entry = node('task', 'open', ['work'], 'core');

    expect(kanbanCardColorClass(entry, 'category', categoryPalette)).toBe('kb-card--category-2');
    expect(kanbanCardColorClass(entry, 'importance', categoryPalette)).toBe('kb-card--priority-core');
    expect(kanbanCardColorClass(entry, 'classification', categoryPalette)).toMatch(/^kb-card--classification-/);
  });
});

function node(
  title: string,
  status: NoteStatus | undefined,
  categories: string[],
  priority: Priority,
  updated = '2024-01-01T00:00:00Z',
): GraphAnalysisNode {
  return {
    id: title,
    type: 'note',
    title,
    summary: '',
    categories,
    status,
    classification: 'todo',
    priority,
    starred: false,
    created: '2024-01-01T00:00:00Z',
    updated,
    x: 0,
    y: 0,
    outlier: false,
    no_semantic_signal: false,
  };
}
