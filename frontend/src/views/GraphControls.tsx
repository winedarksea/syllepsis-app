import { Icon } from '../components/Icon';
import { useStore } from '../lib/store';
import type { GraphMode } from '../types';

const MODES: { id: GraphMode; label: string; description: string }[] = [
  { id: 'categories', label: 'Categories', description: 'Your declared organization' },
  { id: 'pillars', label: 'Pillars', description: 'Broad semantic themes' },
  { id: 'communities', label: 'Communities', description: 'Tightly connected interests' },
  { id: 'density', label: 'Density', description: 'Established regions and outliers' },
];

interface GraphControlsProps {
  visibleSemanticEdges: number;
}

export function GraphControls({ visibleSemanticEdges }: GraphControlsProps) {
  const store = useStore();
  const relevantNeighbors = store.graphMode === 'pillars'
    ? store.graphPillarsNeighbors
    : store.graphMode === 'communities'
      ? store.graphCommunitiesNeighbors
      : store.graphDensityNeighbors;
  const setRelevantNeighbors = store.graphMode === 'pillars'
    ? store.setGraphPillarsNeighbors
    : store.graphMode === 'communities'
      ? store.setGraphCommunitiesNeighbors
      : store.setGraphDensityNeighbors;

  return (
    <header className="gv-toolbar">
      <div className="gv-toolbar-primary">
        <div className="gv-heading">
          <h2 className="gv-title">Graph</h2>
          <span className="gv-mode-description">
            {MODES.find((mode) => mode.id === store.graphMode)?.description}
          </span>
        </div>

        <div className="gv-mode-segments" role="group" aria-label="Graph organization">
          {MODES.map((mode) => (
            <button
              type="button"
              key={mode.id}
              className={store.graphMode === mode.id ? 'active' : ''}
              aria-pressed={store.graphMode === mode.id}
              onClick={() => store.setGraphMode(mode.id)}
            >
              {mode.label}
            </button>
          ))}
        </div>
        <label className="gv-mode-select">
          <span>Organization</span>
          <select
            value={store.graphMode}
            onChange={(event) => store.setGraphMode(event.target.value as GraphMode)}
          >
            {MODES.map((mode) => <option key={mode.id} value={mode.id}>{mode.label}</option>)}
          </select>
        </label>

        <label className="gv-title-control">
          <span>Show all titles</span>
          <button
            type="button"
            role="switch"
            aria-checked={store.showAllGraphTitles}
            className={`gv-switch${store.showAllGraphTitles ? ' on' : ''}`}
            onClick={() => store.setShowAllGraphTitles(!store.showAllGraphTitles)}
          >
            <span className="gv-switch-knob" />
          </button>
        </label>
      </div>

      <div className="gv-toolbar-secondary">
        <label className="gv-threshold-control">
          <span>Similarity</span>
          <input
            type="range"
            min="0.05"
            max="0.95"
            step="0.01"
            value={store.graphSimilarityThreshold}
            aria-valuetext={`${Math.round(store.graphSimilarityThreshold * 100)} percent, ${visibleSemanticEdges} edges`}
            onChange={(event) => store.setGraphSimilarityThreshold(Number(event.target.value))}
          />
          <output>{Math.round(store.graphSimilarityThreshold * 100)}%</output>
          <span className="gv-edge-count">{visibleSemanticEdges} edges</span>
        </label>
        {store.graphMode !== 'categories' && (
          <button
            type="button"
            className="gv-advanced-toggle"
            aria-expanded={store.graphAdvancedOpen}
            onClick={() => store.setGraphAdvancedOpen(!store.graphAdvancedOpen)}
          >
            <Icon name={store.graphAdvancedOpen ? 'expand_less' : 'tune'} size={18} />
            Advanced
          </button>
        )}
      </div>

      {store.graphAdvancedOpen && store.graphMode !== 'categories' && (
        <div className="gv-advanced-panel">
          <GraphNumberControl
            label="UMAP neighbors"
            value={relevantNeighbors}
            min={store.graphMode === 'communities' ? 3 : 5}
            max={100}
            step={1}
            onChange={setRelevantNeighbors}
          />
          {store.graphMode === 'pillars' && (
            <GraphNumberControl label="Pillars" value={store.graphKmeansK} min={2} max={12} step={1} onChange={store.setGraphKmeansK} />
          )}
          {store.graphMode === 'communities' && (
            <GraphNumberControl label="Resolution" value={store.graphLouvainResolution} min={0.25} max={2} step={0.05} onChange={store.setGraphLouvainResolution} />
          )}
          {store.graphMode === 'density' && (
            <GraphNumberControl label="Minimum cluster" value={store.graphHdbscanMinClusterSize} min={2} max={50} step={1} onChange={store.setGraphHdbscanMinClusterSize} />
          )}
        </div>
      )}
    </header>
  );
}

function GraphNumberControl({ label, value, min, max, step, onChange }: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (value: number) => void;
}) {
  return (
    <label className="gv-number-control">
      <span>{label}</span>
      <input type="number" value={value} min={min} max={max} step={step} onChange={(event) => onChange(Number(event.target.value))} />
    </label>
  );
}
