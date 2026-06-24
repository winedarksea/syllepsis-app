import { useEffect, useMemo, useRef, useState } from 'react';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import type { GraphAnalysisRequest, GraphAnalysisResult } from '../types';
import { GraphCanvas } from './GraphCanvas';
import { GraphControls } from './GraphControls';
import { filterSemanticEdges } from './graphGeometry';
import './GraphView.css';

export function GraphView() {
  const store = useStore();
  const [result, setResult] = useState<GraphAnalysisResult | null>(null);
  const [completedRequestKey, setCompletedRequestKey] = useState('');
  const [requestError, setRequestError] = useState<{ key: string; message: string } | null>(null);
  const requestSequence = useRef(0);

  const request = useMemo<GraphAnalysisRequest>(() => ({
    mode: store.graphMode,
    umap_neighbors: store.graphMode === 'pillars'
      ? store.graphPillarsNeighbors
      : store.graphMode === 'communities'
        ? store.graphCommunitiesNeighbors
        : store.graphMode === 'density'
          ? store.graphDensityNeighbors
          : 15,
    kmeans_k: store.graphKmeansK,
    louvain_resolution: store.graphLouvainResolution,
    hdbscan_min_cluster_size: store.graphHdbscanMinClusterSize,
  }), [
    store.graphMode,
    store.graphPillarsNeighbors,
    store.graphCommunitiesNeighbors,
    store.graphDensityNeighbors,
    store.graphKmeansK,
    store.graphLouvainResolution,
    store.graphHdbscanMinClusterSize,
  ]);
  const requestKey = useMemo(() => JSON.stringify(request), [request]);

  useEffect(() => {
    const sequence = ++requestSequence.current;
    api.graphAnalysis(request)
      .then((nextResult) => {
        if (requestSequence.current === sequence) {
          setResult(nextResult);
          setRequestError(null);
          setCompletedRequestKey(requestKey);
        }
      })
      .catch((nextError) => {
        if (requestSequence.current === sequence) {
          setRequestError({ key: requestKey, message: String(nextError) });
          setCompletedRequestKey(requestKey);
        }
      });
  }, [request, requestKey]);

  const loading = completedRequestKey !== requestKey;
  const error = requestError?.key === requestKey ? requestError.message : null;

  const visibleSemanticEdges = useMemo(
    () => filterSemanticEdges(result?.semantic_edges ?? [], store.graphSimilarityThreshold),
    [result, store.graphSimilarityThreshold],
  );

  if (!result && loading) return <div className="gv-state">Mapping your notes…</div>;
  if (!result && error) return <div className="gv-state gv-error">{error}</div>;
  if (!result || result.nodes.length === 0) return <div className="gv-state">No notes to graph yet.</div>;

  return (
    <div className="gv-root">
      <GraphControls visibleSemanticEdges={visibleSemanticEdges.length} />
      {error && <div className="gv-error-banner">{error}</div>}
      <div className="gv-provider-note">
        {result.provider.semantic
          ? `Semantic layout · ${result.provider.id}`
          : 'Lexical fallback · download the embedding model for deeper semantic relationships'}
      </div>
      <GraphCanvas
        result={result}
        semanticEdges={visibleSemanticEdges}
        showAllTitles={store.showAllGraphTitles}
        loading={loading}
        onOpenNote={store.openEditor}
      />
    </div>
  );
}
