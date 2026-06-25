import { Icon } from '../components/Icon';
import { useStore } from '../lib/store';
import type {
  ClustersPreset,
} from '../lib/store';
import type { GraphMode, TimelineDateField, TimelineGranularity } from '../types';

type TopMode = 'categories' | 'clusters' | 'timeline';

const TOP_MODES: { id: TopMode; label: string; description: string }[] = [
  { id: 'categories', label: 'Categories', description: 'Your declared organization' },
  { id: 'clusters', label: 'Clusters', description: 'Emergent groups from note embeddings' },
  { id: 'timeline', label: 'Timeline', description: 'Notes laid out by date' },
];

// The three clustering algorithms, surfaced as presets under "Clusters".
const CLUSTER_PRESETS: { id: ClustersPreset; label: string; algorithm: string }[] = [
  { id: 'pillars', label: 'Themes', algorithm: 'k-means' },
  { id: 'communities', label: 'Communities', algorithm: 'Louvain' },
  { id: 'density', label: 'Regions', algorithm: 'HDBSCAN' },
];

const DATE_FIELDS: { id: TimelineDateField; label: string }[] = [
  { id: 'created', label: 'Created' },
  { id: 'updated', label: 'Updated' },
  { id: 'scheduled', label: 'Scheduled' },
  { id: 'completed', label: 'Completed' },
];

const GRANULARITIES: { id: TimelineGranularity; label: string }[] = [
  { id: 'auto', label: 'Auto' },
  { id: 'hour', label: 'Hour' },
  { id: 'day', label: 'Day' },
  { id: 'month', label: 'Month' },
  { id: 'year', label: 'Year' },
];

function topModeOf(mode: GraphMode): TopMode {
  if (mode === 'categories') return 'categories';
  if (mode === 'timeline') return 'timeline';
  return 'clusters';
}

interface GraphControlsProps {
  visibleSemanticEdges: number;
}

export function GraphControls({ visibleSemanticEdges }: GraphControlsProps) {
  const store = useStore();
  const topMode = topModeOf(store.graphMode);

  const selectTopMode = (next: TopMode) => {
    if (next === 'categories') store.setGraphMode('categories');
    else if (next === 'timeline') store.setGraphMode('timeline');
    else store.setGraphMode(store.clustersPreset);
  };

  const selectPreset = (preset: ClustersPreset) => {
    store.setClustersPreset(preset);
    store.setGraphMode(preset);
  };

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
            {TOP_MODES.find((mode) => mode.id === topMode)?.description}
          </span>
        </div>

        <div className="gv-mode-segments" role="group" aria-label="Graph organization">
          {TOP_MODES.map((mode) => (
            <button
              type="button"
              key={mode.id}
              className={topMode === mode.id ? 'active' : ''}
              aria-pressed={topMode === mode.id}
              onClick={() => selectTopMode(mode.id)}
            >
              {mode.label}
            </button>
          ))}
        </div>
        <label className="gv-mode-select">
          <span>Organization</span>
          <select
            value={topMode}
            onChange={(event) => selectTopMode(event.target.value as TopMode)}
          >
            {TOP_MODES.map((mode) => <option key={mode.id} value={mode.id}>{mode.label}</option>)}
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

      {topMode === 'clusters' && (
        <div className="gv-preset-segments">
          <span className="gv-preset-label">Method</span>
          <div className="gv-preset-group" role="group" aria-label="Cluster method">
            {CLUSTER_PRESETS.map((preset) => (
              <button
                type="button"
                key={preset.id}
                className={store.graphMode === preset.id ? 'active' : ''}
                aria-pressed={store.graphMode === preset.id}
                onClick={() => selectPreset(preset.id)}
              >
                {preset.label}
                <span className="gv-preset-algorithm">{preset.algorithm}</span>
              </button>
            ))}
          </div>
        </div>
      )}

      {topMode === 'timeline' ? (
        <div className="gv-timeline-panel">
          <label className="gv-timeline-control">
            <span>Date</span>
            <select
              value={store.timelinePrimaryDate}
              onChange={(event) => store.setTimelinePrimaryDate(event.target.value as TimelineDateField)}
            >
              {DATE_FIELDS.map((field) => <option key={field.id} value={field.id}>{field.label}</option>)}
            </select>
          </label>
          <label className="gv-timeline-control">
            <span>Fallback</span>
            <select
              value={store.timelineFallbackDate ?? ''}
              onChange={(event) =>
                store.setTimelineFallbackDate(event.target.value === '' ? null : event.target.value as TimelineDateField)}
            >
              <option value="">(none)</option>
              {DATE_FIELDS.map((field) => <option key={field.id} value={field.id}>{field.label}</option>)}
            </select>
          </label>
          <label className="gv-timeline-control">
            <span>Aggregation</span>
            <select
              value={store.timelineGranularity}
              onChange={(event) => store.setTimelineGranularity(event.target.value as TimelineGranularity)}
            >
              {GRANULARITIES.map((g) => <option key={g.id} value={g.id}>{g.label}</option>)}
            </select>
          </label>
          <div className="gv-timeline-color" role="group" aria-label="Color by">
            <span>Color</span>
            <button
              type="button"
              className={store.timelineColorBy === 'category' ? 'active' : ''}
              aria-pressed={store.timelineColorBy === 'category'}
              onClick={() => store.setTimelineColorBy('category')}
            >
              Category
            </button>
            <button
              type="button"
              className={store.timelineColorBy === 'cluster' ? 'active' : ''}
              aria-pressed={store.timelineColorBy === 'cluster'}
              onClick={() => store.setTimelineColorBy('cluster')}
            >
              Cluster
            </button>
          </div>
        </div>
      ) : (
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
          {topMode === 'clusters' && (
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
      )}

      {store.graphAdvancedOpen && topMode === 'clusters' && (
        <div className="gv-advanced-panel">
          <label className="gv-title-control">
            <span>Automatic defaults</span>
            <button
              type="button"
              role="switch"
              aria-checked={store.graphAutomaticClusterDefaults}
              className={`gv-switch${store.graphAutomaticClusterDefaults ? ' on' : ''}`}
              onClick={() =>
                store.setGraphAutomaticClusterDefaults(!store.graphAutomaticClusterDefaults)}
            >
              <span className="gv-switch-knob" />
            </button>
          </label>
          <GraphNumberControl
            label="UMAP neighbors"
            value={relevantNeighbors}
            min={store.graphMode === 'communities' ? 3 : 5}
            max={100}
            step={1}
            disabled={store.graphAutomaticClusterDefaults}
            onChange={setRelevantNeighbors}
          />
          {store.graphMode === 'pillars' && (
            <GraphNumberControl label="Themes" value={store.graphKmeansK} min={2} max={12} step={1} disabled={store.graphAutomaticClusterDefaults} onChange={store.setGraphKmeansK} />
          )}
          {store.graphMode === 'communities' && (
            <GraphNumberControl label="Resolution" value={store.graphLouvainResolution} min={0.25} max={2} step={0.05} disabled={store.graphAutomaticClusterDefaults} onChange={store.setGraphLouvainResolution} />
          )}
          {store.graphMode === 'density' && (
            <GraphNumberControl label="Minimum cluster" value={store.graphHdbscanMinClusterSize} min={2} max={50} step={1} disabled={store.graphAutomaticClusterDefaults} onChange={store.setGraphHdbscanMinClusterSize} />
          )}
        </div>
      )}
    </header>
  );
}

function GraphNumberControl({ label, value, min, max, step, disabled, onChange }: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  disabled?: boolean;
  onChange: (value: number) => void;
}) {
  return (
    <label className="gv-number-control">
      <span>{label}</span>
      <input type="number" value={value} min={min} max={max} step={step} disabled={disabled} onChange={(event) => onChange(Number(event.target.value))} />
    </label>
  );
}
