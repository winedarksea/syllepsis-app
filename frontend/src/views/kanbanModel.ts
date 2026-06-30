import type { GraphAnalysisNode, KanbanColorBy, NoteStatus, Priority } from '../types';

export type KanbanSectionId = 'todo' | 'active' | 'done';
export type KanbanDropStatus = 'open' | 'active' | 'done';

export const KANBAN_SECTIONS: { id: KanbanSectionId; label: string; dropStatus: KanbanDropStatus }[] = [
  { id: 'todo', label: 'To-Do', dropStatus: 'open' },
  { id: 'active', label: 'In-Progress', dropStatus: 'active' },
  { id: 'done', label: 'Done', dropStatus: 'done' },
];

export const NOTE_STATUS_OPTIONS: Array<NoteStatus | null> = [
  null,
  'open',
  'active',
  'needs_clarification',
  'deferred',
  'done',
  'cancelled',
];

export const PRIORITY_OPTIONS: Priority[] = ['standard', 'important', 'core'];

export const NOTE_STATUS_LABELS: Record<NoteStatus | 'none', string> = {
  none: 'No Status',
  open: 'Open',
  active: 'Active',
  needs_clarification: 'Needs Clarification',
  deferred: 'Deferred',
  cancelled: 'Cancelled',
  done: 'Done',
};

const TODO_STATUS_ORDER = new Map<NoteStatus | undefined, number>([
  [undefined, 0],
  ['open', 1],
  ['deferred', 2],
  ['needs_clarification', 3],
]);

const DONE_STATUS_ORDER = new Map<NoteStatus | undefined, number>([
  ['done', 0],
  ['cancelled', 1],
]);

export interface KanbanFilters {
  selectedCategories: string[];
  selectedPriorities: Priority[];
  showNoStatus: boolean;
}

export function kanbanSectionForStatus(status?: NoteStatus): KanbanSectionId {
  if (status === 'active') return 'active';
  if (status === 'done' || status === 'cancelled') return 'done';
  return 'todo';
}

export function filterKanbanNodes(
  nodes: GraphAnalysisNode[],
  filters: KanbanFilters,
): GraphAnalysisNode[] {
  const categorySet = new Set(filters.selectedCategories);
  const prioritySet = new Set(filters.selectedPriorities);
  return nodes.filter((node) => {
    if (!filters.showNoStatus && !node.status) return false;
    if (categorySet.size > 0 && !node.categories.some((category) => categorySet.has(category))) {
      return false;
    }
    return prioritySet.has(node.priority);
  });
}

export function groupKanbanNodes(
  nodes: GraphAnalysisNode[],
): Record<KanbanSectionId, GraphAnalysisNode[]> {
  const grouped: Record<KanbanSectionId, GraphAnalysisNode[]> = {
    todo: [],
    active: [],
    done: [],
  };
  for (const node of nodes) grouped[kanbanSectionForStatus(node.status)].push(node);
  grouped.todo.sort(compareTodoNodes);
  grouped.active.sort(compareByUpdatedDescending);
  grouped.done.sort(compareDoneNodes);
  return grouped;
}

export function kanbanCardColorClass(
  node: GraphAnalysisNode,
  colorBy: KanbanColorBy,
  categoryPalette: Map<string, number>,
): string {
  if (colorBy === 'importance') return `kb-card--priority-${node.priority}`;
  if (colorBy === 'category') {
    const firstCategory = node.categories[0];
    if (!firstCategory) return 'kb-card--category-none';
    return `kb-card--category-${categoryPalette.get(firstCategory) ?? 0}`;
  }
  return `kb-card--classification-${classificationColorIndex(node.classification)}`;
}

function compareTodoNodes(left: GraphAnalysisNode, right: GraphAnalysisNode): number {
  const statusDelta = (TODO_STATUS_ORDER.get(left.status) ?? 99) - (TODO_STATUS_ORDER.get(right.status) ?? 99);
  return statusDelta || compareByUpdatedDescending(left, right);
}

function compareDoneNodes(left: GraphAnalysisNode, right: GraphAnalysisNode): number {
  const statusDelta = (DONE_STATUS_ORDER.get(left.status) ?? 99) - (DONE_STATUS_ORDER.get(right.status) ?? 99);
  return statusDelta || compareByUpdatedDescending(left, right);
}

function compareByUpdatedDescending(left: GraphAnalysisNode, right: GraphAnalysisNode): number {
  return Date.parse(right.updated) - Date.parse(left.updated) || left.title.localeCompare(right.title);
}

function classificationColorIndex(classification: string): number {
  let hash = 0;
  for (let index = 0; index < classification.length; index += 1) {
    hash = (hash + classification.charCodeAt(index) * (index + 1)) % 6;
  }
  return hash;
}
