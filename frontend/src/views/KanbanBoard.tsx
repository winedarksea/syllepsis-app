import { useMemo, useRef, useState } from 'react';
import type { DragEvent, PointerEvent, ReactNode } from 'react';
import { Icon } from '../components/Icon';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import type { GraphAnalysisNode, KanbanColorBy, NoteDto, NoteStatus, Priority } from '../types';
import {
  filterKanbanNodes,
  groupKanbanNodes,
  kanbanCardColorClass,
  KANBAN_SECTIONS,
  NOTE_STATUS_LABELS,
  NOTE_STATUS_OPTIONS,
  PRIORITY_OPTIONS,
  type KanbanSectionId,
} from './kanbanModel';
import './KanbanBoard.css';

const STATUS_ICONS: Record<NoteStatus | 'none', string> = {
  none: 'radio_button_unchecked',
  open: 'radio_button_unchecked',
  active: 'play_circle',
  needs_clarification: 'help',
  deferred: 'schedule',
  cancelled: 'block',
  done: 'task_alt',
};

const COLOR_LABELS: Record<KanbanColorBy, string> = {
  classification: 'Classification',
  category: 'Category',
  importance: 'Importance',
};

const POINTER_DRAG_START_DISTANCE_PX = 6;
const KANBAN_DROP_SECTION_ATTRIBUTE = 'data-kanban-drop-section';

interface KanbanBoardProps {
  nodes: GraphAnalysisNode[];
  loading: boolean;
  onOpenNote: (id: string) => void;
  onWorkflowUpdated: (note: NoteDto) => void;
}

interface PointerDragState {
  nodeId: string;
  title: string;
  pointerId: number;
  startX: number;
  startY: number;
  currentX: number;
  currentY: number;
  dragging: boolean;
  targetSection: KanbanSectionId | null;
}

export function KanbanBoard({ nodes, loading, onOpenNote, onWorkflowUpdated }: KanbanBoardProps) {
  const store = useStore();
  const [activeSection, setActiveSection] = useState<KanbanSectionId>('todo');
  const [dragOverSection, setDragOverSection] = useState<KanbanSectionId | null>(null);
  const [busyNoteId, setBusyNoteId] = useState<string | null>(null);
  const [openMenuId, setOpenMenuId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pointerDragPreview, setPointerDragPreview] = useState<PointerDragState | null>(null);
  const pointerDragStateRef = useRef<PointerDragState | null>(null);

  const categories = useMemo(
    () => Array.from(new Set(nodes.flatMap((node) => node.categories))).sort((a, b) => a.localeCompare(b)),
    [nodes],
  );
  const categoryPalette = useMemo(
    () => new Map(categories.map((category, index) => [category, index % 6])),
    [categories],
  );
  const filteredNodes = useMemo(
    () => filterKanbanNodes(nodes, {
      selectedCategories: store.kanbanSelectedCategories,
      selectedPriorities: store.kanbanSelectedPriorities,
      showNoStatus: store.kanbanShowNoStatus,
    }),
    [
      nodes,
      store.kanbanSelectedCategories,
      store.kanbanSelectedPriorities,
      store.kanbanShowNoStatus,
    ],
  );
  const grouped = useMemo(() => groupKanbanNodes(filteredNodes), [filteredNodes]);

  const setCategoryEnabled = (category: string, enabled: boolean) => {
    const current = new Set(store.kanbanSelectedCategories.length === 0
      ? categories
      : store.kanbanSelectedCategories);
    if (enabled) current.add(category);
    else current.delete(category);
    const selected = Array.from(current).sort((a, b) => a.localeCompare(b));
    store.setKanbanSelectedCategories(selected.length === categories.length ? [] : selected);
  };

  const setPriorityEnabled = (priority: Priority, enabled: boolean) => {
    const current = new Set(store.kanbanSelectedPriorities);
    if (enabled) current.add(priority);
    else current.delete(priority);
    store.setKanbanSelectedPriorities(PRIORITY_OPTIONS.filter((option) => current.has(option)));
  };

  const updateStatus = async (nodeId: string, status: NoteStatus | null) => {
    setBusyNoteId(nodeId);
    setError(null);
    try {
      const updated = await api.setNoteWorkflowStatus(nodeId, status, localDateString());
      onWorkflowUpdated(updated);
      setOpenMenuId(null);
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setBusyNoteId(null);
    }
  };

  const handleDrop = (section: KanbanSectionId, event: DragEvent) => {
    event.preventDefault();
    setDragOverSection(null);
    const nodeId = event.dataTransfer.getData('text/plain');
    const target = KANBAN_SECTIONS.find((entry) => entry.id === section)?.dropStatus;
    if (nodeId && target) void updateStatus(nodeId, target);
  };

  const beginPointerDrag = (node: GraphAnalysisNode, event: PointerEvent<HTMLElement>) => {
    if (event.pointerType === 'mouse' && event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture?.(event.pointerId);
    const nextDragState: PointerDragState = {
      nodeId: node.id,
      title: node.title,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      currentX: event.clientX,
      currentY: event.clientY,
      dragging: false,
      targetSection: null,
    };
    pointerDragStateRef.current = nextDragState;
    setPointerDragPreview(nextDragState);
  };

  const updatePointerDrag = (event: PointerEvent<HTMLElement>) => {
    const currentDragState = pointerDragStateRef.current;
    if (!currentDragState || currentDragState.pointerId !== event.pointerId) return;
    event.preventDefault();
    event.stopPropagation();

    const distanceFromStart = Math.hypot(
      event.clientX - currentDragState.startX,
      event.clientY - currentDragState.startY,
    );
    const dragging = currentDragState.dragging || distanceFromStart >= POINTER_DRAG_START_DISTANCE_PX;
    const targetSection = dragging ? sectionUnderPointer(event.clientX, event.clientY) : null;
    const nextDragState = {
      ...currentDragState,
      currentX: event.clientX,
      currentY: event.clientY,
      dragging,
      targetSection,
    };
    pointerDragStateRef.current = nextDragState;
    setPointerDragPreview(nextDragState);
    setDragOverSection(targetSection);
  };

  const finishPointerDrag = (event: PointerEvent<HTMLElement>) => {
    const currentDragState = pointerDragStateRef.current;
    if (!currentDragState || currentDragState.pointerId !== event.pointerId) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    pointerDragStateRef.current = null;
    setPointerDragPreview(null);
    setDragOverSection(null);

    if (!currentDragState.dragging || !currentDragState.targetSection) return;
    const target = KANBAN_SECTIONS.find((entry) => entry.id === currentDragState.targetSection)?.dropStatus;
    if (target) void updateStatus(currentDragState.nodeId, target);
  };

  const cancelPointerDrag = (event: PointerEvent<HTMLElement>) => {
    const currentDragState = pointerDragStateRef.current;
    if (!currentDragState || currentDragState.pointerId !== event.pointerId) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    pointerDragStateRef.current = null;
    setPointerDragPreview(null);
    setDragOverSection(null);
  };

  const selectedCategoryLabel = store.kanbanSelectedCategories.length === 0
    ? 'All Categories'
    : `${store.kanbanSelectedCategories.length} Categories`;
  const selectedPriorityLabel = store.kanbanSelectedPriorities.length === PRIORITY_OPTIONS.length
    ? 'All Importance'
    : `${store.kanbanSelectedPriorities.length} Importance`;

  return (
    <div className={`kb-root${loading ? ' loading' : ''}`}>
      <div className="kb-controls">
        <FilterMenu label={selectedCategoryLabel} icon="sell">
          <button
            type="button"
            className="kb-menu-action"
            onClick={() => store.setKanbanSelectedCategories([])}
          >
            All categories
          </button>
          {categories.map((category) => (
            <label key={category} className="kb-menu-check">
              <input
                type="checkbox"
                checked={store.kanbanSelectedCategories.length === 0 || store.kanbanSelectedCategories.includes(category)}
                onChange={(event) => setCategoryEnabled(category, event.target.checked)}
              />
              <span>{category}</span>
            </label>
          ))}
        </FilterMenu>

        <FilterMenu label={selectedPriorityLabel} icon="priority_high">
          {PRIORITY_OPTIONS.map((priority) => (
            <label key={priority} className="kb-menu-check">
              <input
                type="checkbox"
                checked={store.kanbanSelectedPriorities.includes(priority)}
                onChange={(event) => setPriorityEnabled(priority, event.target.checked)}
              />
              <span>{humanize(priority)}</span>
            </label>
          ))}
        </FilterMenu>

        <label className="kb-switch-control">
          <span>No Status</span>
          <button
            type="button"
            role="switch"
            aria-checked={store.kanbanShowNoStatus}
            className={`gv-switch${store.kanbanShowNoStatus ? ' on' : ''}`}
            onClick={() => store.setKanbanShowNoStatus(!store.kanbanShowNoStatus)}
          >
            <span className="gv-switch-knob" />
          </button>
        </label>

        <label className="kb-select-control">
          <span>Color</span>
          <select
            value={store.kanbanColorBy}
            onChange={(event) => store.setKanbanColorBy(event.target.value as KanbanColorBy)}
          >
            {Object.entries(COLOR_LABELS).map(([value, label]) => (
              <option key={value} value={value}>{label}</option>
            ))}
          </select>
        </label>
      </div>

      <div className="kb-section-tabs" role="group" aria-label="Kanban sections">
        {KANBAN_SECTIONS.map((section) => (
          <button
            key={section.id}
            type="button"
            className={activeSection === section.id ? 'active' : ''}
            aria-pressed={activeSection === section.id}
            onClick={() => setActiveSection(section.id)}
          >
            {section.label}
            <span>{grouped[section.id].length}</span>
          </button>
        ))}
      </div>

      {error && <div className="kb-error">{error}</div>}

      <div className="kb-board">
        {KANBAN_SECTIONS.map((section) => (
          <section
            key={section.id}
            className={`kb-column${dragOverSection === section.id ? ' drag-over' : ''}${activeSection === section.id ? ' active' : ''}`}
            data-kanban-drop-section={section.id}
            onDragOver={(event) => {
              event.preventDefault();
              setDragOverSection(section.id);
            }}
            onDragLeave={() => setDragOverSection(null)}
            onDrop={(event) => handleDrop(section.id, event)}
          >
            <div className="kb-column-head">
              <h3>{section.label}</h3>
              <span>{grouped[section.id].length}</span>
            </div>
            <div className="kb-card-list">
              {grouped[section.id].map((node) => (
                <KanbanCard
                  key={node.id}
                  node={node}
                  busy={busyNoteId === node.id}
                  menuOpen={openMenuId === node.id}
                  colorClass={kanbanCardColorClass(node, store.kanbanColorBy, categoryPalette)}
                  onOpen={() => onOpenNote(node.id)}
                  onToggleMenu={() => setOpenMenuId((current) => current === node.id ? null : node.id)}
                  onStatusChange={(status) => updateStatus(node.id, status)}
                  onPointerDragStart={(event) => beginPointerDrag(node, event)}
                  onPointerDragMove={updatePointerDrag}
                  onPointerDragEnd={finishPointerDrag}
                  onPointerDragCancel={cancelPointerDrag}
                />
              ))}
              {grouped[section.id].length === 0 && (
                <div className="kb-empty-column">No notes</div>
              )}
            </div>
          </section>
        ))}
      </div>
      {pointerDragPreview?.dragging && (
        <div
          className="kb-drag-preview"
          style={{
            transform: `translate3d(${pointerDragPreview.currentX}px, ${pointerDragPreview.currentY}px, 0)`,
          }}
        >
          {pointerDragPreview.title}
        </div>
      )}
    </div>
  );
}

function KanbanCard({
  node,
  busy,
  menuOpen,
  colorClass,
  onOpen,
  onToggleMenu,
  onStatusChange,
  onPointerDragStart,
  onPointerDragMove,
  onPointerDragEnd,
  onPointerDragCancel,
}: {
  node: GraphAnalysisNode;
  busy: boolean;
  menuOpen: boolean;
  colorClass: string;
  onOpen: () => void;
  onToggleMenu: () => void;
  onStatusChange: (status: NoteStatus | null) => void;
  onPointerDragStart: (event: PointerEvent<HTMLElement>) => void;
  onPointerDragMove: (event: PointerEvent<HTMLElement>) => void;
  onPointerDragEnd: (event: PointerEvent<HTMLElement>) => void;
  onPointerDragCancel: (event: PointerEvent<HTMLElement>) => void;
}) {
  const statusKey = node.status ?? 'none';
  return (
    <article
      className={`kb-card ${colorClass}${node.status === 'cancelled' ? ' kb-card--cancelled' : ''}`}
      role="button"
      tabIndex={0}
      aria-busy={busy}
      onClick={onOpen}
      onKeyDown={(event) => {
        if (event.key === 'Enter') onOpen();
      }}
    >
      <div className="kb-card-topline">
        <span
          className="kb-drag-handle"
          draggable
          role="button"
          tabIndex={0}
          title="Drag note"
          aria-label={`Drag ${node.title}`}
          onClick={(event) => event.stopPropagation()}
          onPointerDown={onPointerDragStart}
          onPointerMove={onPointerDragMove}
          onPointerUp={onPointerDragEnd}
          onPointerCancel={onPointerDragCancel}
          onDragStart={(event) => {
            event.dataTransfer.effectAllowed = 'move';
            event.dataTransfer.setData('text/plain', node.id);
          }}
        >
          <Icon name="drag_indicator" size={18} />
        </span>
        <span className="kb-status-badge">
          <Icon name={STATUS_ICONS[statusKey]} size={15} />
          {NOTE_STATUS_LABELS[statusKey]}
        </span>
        {node.starred && <Icon name="star" size={15} fill className="kb-star" title="Starred" />}
        <button
          type="button"
          className="kb-status-menu-button"
          aria-expanded={menuOpen}
          aria-label={`Change status for ${node.title}`}
          onClick={(event) => {
            event.stopPropagation();
            onToggleMenu();
          }}
        >
          <Icon name="more_horiz" size={18} />
        </button>
        {menuOpen && (
          <div className="kb-status-menu" onClick={(event) => event.stopPropagation()}>
            {NOTE_STATUS_OPTIONS.map((status) => {
              const key = status ?? 'none';
              return (
                <button
                  key={key}
                  type="button"
                  className={node.status === status ? 'active' : ''}
                  onClick={() => onStatusChange(status)}
                >
                  <Icon name={STATUS_ICONS[key]} size={15} />
                  {NOTE_STATUS_LABELS[key]}
                </button>
              );
            })}
          </div>
        )}
      </div>
      <h4 className="kb-card-title">{node.title}</h4>
      {node.summary && <p className="kb-card-summary">{node.summary}</p>}
      <div className="kb-card-meta">
        <span>{humanize(node.classification)}</span>
        {node.type !== 'note' && <span>{node.type}</span>}
        <span>{humanize(node.priority)}</span>
      </div>
      {node.categories.length > 0 && (
        <div className="kb-card-tags">
          {node.categories.map((category) => <span key={category}>#{category}</span>)}
        </div>
      )}
      <time className="kb-card-date" dateTime={node.updated}>
        Updated {formatShortDate(node.updated)}
      </time>
    </article>
  );
}

function FilterMenu({ label, icon, children }: { label: string; icon: string; children: ReactNode }) {
  return (
    <details className="kb-filter-menu">
      <summary>
        <Icon name={icon} size={16} />
        {label}
      </summary>
      <div className="kb-filter-panel">{children}</div>
    </details>
  );
}

function localDateString(): string {
  const now = new Date();
  const month = String(now.getMonth() + 1).padStart(2, '0');
  const day = String(now.getDate()).padStart(2, '0');
  return `${now.getFullYear()}-${month}-${day}`;
}

function sectionUnderPointer(clientX: number, clientY: number): KanbanSectionId | null {
  const elementUnderPointer = document.elementFromPoint(clientX, clientY);
  const dropSection = elementUnderPointer?.closest<HTMLElement>(`[${KANBAN_DROP_SECTION_ATTRIBUTE}]`);
  const sectionId = dropSection?.dataset.kanbanDropSection;
  return KANBAN_SECTIONS.some((section) => section.id === sectionId)
    ? sectionId as KanbanSectionId
    : null;
}

function humanize(value: string): string {
  return value.replace(/_/g, ' ').replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function formatShortDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}
