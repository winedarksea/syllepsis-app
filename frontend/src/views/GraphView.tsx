import { useEffect, useMemo, useRef, useState } from 'react';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import type { GraphAnalysisRequest, GraphAnalysisResult } from '../types';
import { GraphCanvas } from './GraphCanvas';
import { GraphControls } from './GraphControls';
import { filterSemanticEdges } from './graphGeometry';
import './GraphView.css';

const EMBEDDINGGEMMA_MODEL_ID = 'embeddinggemma-300m';

export function GraphView() {
  const store = useStore();
  const [result, setResult] = useState<GraphAnalysisResult | null>(null);
  const [completedRequestKey, setCompletedRequestKey] = useState('');
  const [requestError, setRequestError] = useState<{ key: string; message: string } | null>(null);
  const [modelDownloadError, setModelDownloadError] = useState<string | null>(null);
  const [modelDownloadInProgress, setModelDownloadInProgress] = useState(false);
  const [embeddingModelRevision, setEmbeddingModelRevision] = useState(0);
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
  }, [request, requestKey, embeddingModelRevision]);

  const downloadEmbeddingModel = async () => {
    setModelDownloadInProgress(true);
    setModelDownloadError(null);
    try {
      await api.downloadBuiltinModel(EMBEDDINGGEMMA_MODEL_ID);
      setEmbeddingModelRevision((revision) => revision + 1);
    } catch (nextError) {
      setModelDownloadError(String(nextError));
    } finally {
      setModelDownloadInProgress(false);
    }
  };

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
      {(error || modelDownloadError) && (
        <div className="gv-error-banner">{modelDownloadError ?? error}</div>
      )}
      <div className="gv-provider-note">
        {result.provider.semantic
          ? `Semantic layout · ${result.provider.id}`
          : (
            <>
              <span>Lexical fallback · shared words only</span>
              <button
                className="gv-model-download"
                type="button"
                disabled={modelDownloadInProgress}
                onClick={downloadEmbeddingModel}
              >
                {modelDownloadInProgress ? 'Downloading EmbeddingGemma…' : 'Download EmbeddingGemma model'}
              </button>
            </>
          )}
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
