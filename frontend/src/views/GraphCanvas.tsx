import { useMemo, useRef, useState } from 'react';
import type { PointerEvent as ReactPointerEvent, WheelEvent as ReactWheelEvent } from 'react';
import { Icon, useThemeStyle } from '../components/Icon';
import type {
  GraphAnalysisNode, GraphAnalysisResult, GraphSemanticEdge,
} from '../types';
import {
  GRAPH_HEIGHT, GRAPH_WIDTH, graphNodePoint, paddedHullPath, zoomCameraAtPoint,
  type GraphCamera, type GraphPoint,
} from './graphGeometry';

const WEAVE_LIMIT = 140;
const INITIAL_CAMERA: GraphCamera = { x: 0, y: 0, zoom: 1 };

interface GraphCanvasProps {
  result: GraphAnalysisResult;
  semanticEdges: GraphSemanticEdge[];
  showAllTitles: boolean;
  showPriorRelationships?: boolean;
  loading: boolean;
  onOpenNote: (id: string) => void;
}

export function GraphCanvas({
  result,
  semanticEdges,
  showAllTitles,
  showPriorRelationships = true,
  loading,
  onOpenNote,
}: GraphCanvasProps) {
  const themeStyle = useThemeStyle();
  const svgRef = useRef<SVGSVGElement>(null);
  const activePointers = useRef(new Map<number, GraphPoint>());
  const dragState = useRef<{ pointerId: number; x: number; y: number } | null>(null);
  const pinchState = useRef<{ distance: number } | null>(null);
  const suppressClick = useRef(false);
  const [camera, setCamera] = useState(INITIAL_CAMERA);
  const [hoveredNodeId, setHoveredNodeId] = useState<string | null>(null);

  const pointsById = useMemo(
    () => new Map(result.nodes.map((node) => [node.id, graphNodePoint(node)])),
    [result.nodes],
  );
  const nodesByCluster = useMemo(() => {
    const groups = new Map<number, GraphAnalysisNode[]>();
    for (const node of result.nodes) {
      if (node.cluster_id === undefined || node.outlier || node.no_semantic_signal) continue;
      const members = groups.get(node.cluster_id) ?? [];
      members.push(node);
      groups.set(node.cluster_id, members);
    }
    return groups;
  }, [result.nodes]);

  const clientToGraph = (clientX: number, clientY: number): GraphPoint => {
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect) return { x: camera.x, y: camera.y };
    return {
      x: camera.x + (clientX - rect.left) / rect.width * (GRAPH_WIDTH / camera.zoom),
      y: camera.y + (clientY - rect.top) / rect.height * (GRAPH_HEIGHT / camera.zoom),
    };
  };

  const handleWheel = (event: ReactWheelEvent<SVGSVGElement>) => {
    event.preventDefault();
    const factor = Math.exp(-event.deltaY * 0.0015);
    const point = clientToGraph(event.clientX, event.clientY);
    setCamera((current) => zoomCameraAtPoint(current, point, current.zoom * factor));
  };

  const handlePointerDown = (event: ReactPointerEvent<SVGSVGElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId);
    activePointers.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
    suppressClick.current = false;
    if (activePointers.current.size === 1) {
      dragState.current = { pointerId: event.pointerId, x: event.clientX, y: event.clientY };
    } else if (activePointers.current.size === 2) {
      pinchState.current = { distance: pointerDistance([...activePointers.current.values()]) };
      dragState.current = null;
    }
  };

  const handlePointerMove = (event: ReactPointerEvent<SVGSVGElement>) => {
    if (!activePointers.current.has(event.pointerId)) return;
    activePointers.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
    if (activePointers.current.size === 2) {
      const points = [...activePointers.current.values()];
      const distance = pointerDistance(points);
      const midpoint = { x: (points[0].x + points[1].x) / 2, y: (points[0].y + points[1].y) / 2 };
      const previousDistance = pinchState.current?.distance ?? distance;
      const graphMidpoint = clientToGraph(midpoint.x, midpoint.y);
      setCamera((current) =>
        zoomCameraAtPoint(current, graphMidpoint, current.zoom * distance / previousDistance));
      pinchState.current = { distance };
      suppressClick.current = true;
      return;
    }
    const drag = dragState.current;
    const rect = svgRef.current?.getBoundingClientRect();
    if (!drag || drag.pointerId !== event.pointerId || !rect) return;
    const dx = event.clientX - drag.x;
    const dy = event.clientY - drag.y;
    if (Math.hypot(dx, dy) > 3) suppressClick.current = true;
    setCamera((current) => ({
      ...current,
      x: current.x - dx / rect.width * (GRAPH_WIDTH / current.zoom),
      y: current.y - dy / rect.height * (GRAPH_HEIGHT / current.zoom),
    }));
    dragState.current = { pointerId: event.pointerId, x: event.clientX, y: event.clientY };
  };

  const handlePointerEnd = (event: ReactPointerEvent<SVGSVGElement>) => {
    activePointers.current.delete(event.pointerId);
    if (activePointers.current.size < 2) pinchState.current = null;
    if (activePointers.current.size === 1) {
      const [pointerId, point] = [...activePointers.current.entries()][0];
      dragState.current = { pointerId, x: point.x, y: point.y };
    } else if (activePointers.current.size === 0) {
      dragState.current = null;
    }
  };

  const zoomBy = (factor: number) => {
    const center = {
      x: camera.x + GRAPH_WIDTH / camera.zoom / 2,
      y: camera.y + GRAPH_HEIGHT / camera.zoom / 2,
    };
    setCamera((current) => zoomCameraAtPoint(current, center, current.zoom * factor));
  };

  return (
    <div className={`gv-canvas${loading ? ' loading' : ''}`}>
      <svg
        ref={svgRef}
        viewBox={`${camera.x} ${camera.y} ${GRAPH_WIDTH / camera.zoom} ${GRAPH_HEIGHT / camera.zoom}`}
        className="gv-svg"
        onWheel={handleWheel}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerEnd}
        onPointerCancel={handlePointerEnd}
      >
        <g className="gv-cluster-regions">
          {result.clusters.map((cluster) => {
            const members = nodesByCluster.get(cluster.id) ?? [];
            const points = members.map(graphNodePoint);
            if (points.length === 0) return null;
            const labelPoint = regionLabelPoint(points);
            const className = `gv-cluster-region gv-cluster-${cluster.id % 5} gv-pattern-${cluster.id % 3}`;
            return (
              <g key={cluster.id} className={className}>
                {points.length === 1 ? (
                  <circle cx={points[0].x} cy={points[0].y} r={38} />
                ) : points.length === 2 ? (
                  <line x1={points[0].x} y1={points[0].y} x2={points[1].x} y2={points[1].y} />
                ) : (
                  <path d={paddedHullPath(points)} />
                )}
                <text x={labelPoint.x} y={labelPoint.y}>{cluster.label}</text>
              </g>
            );
          })}
        </g>

        <g className="gv-semantic-edges">
          {semanticEdges.map((edge) => {
            const source = pointsById.get(edge.source);
            const target = pointsById.get(edge.target);
            if (!source || !target) return null;
            return (
              <line
                key={`${edge.source}:${edge.target}`}
                x1={source.x}
                y1={source.y}
                x2={target.x}
                y2={target.y}
                style={{ opacity: 0.16 + edge.similarity * 0.52 }}
              />
            );
          })}
        </g>

        {showPriorRelationships && (
          <g className="gv-prior-edges">
            {result.prior_edges.map((edge) => {
              const source = pointsById.get(edge.source);
              const target = pointsById.get(edge.target);
              if (!source || !target) return null;
              const showCasing = themeStyle.graphEdge === 'weave'
                && result.prior_edges.length <= WEAVE_LIMIT;
              return (
                <g key={`${edge.source}:${edge.target}`}>
                  {showCasing && <line x1={source.x} y1={source.y} x2={target.x} y2={target.y} className="gv-edge-casing" />}
                  <line x1={source.x} y1={source.y} x2={target.x} y2={target.y} className="gv-prior-edge" />
                </g>
              );
            })}
          </g>
        )}

        <g className="gv-nodes">
          {result.nodes.map((node) => {
            const point = pointsById.get(node.id)!;
            const active = hoveredNodeId === node.id;
            const clusterClass = node.cluster_id === undefined ? '' : ` gv-node--${node.cluster_id % 5}`;
            return (
              <g
                key={node.id}
                transform={`translate(${point.x} ${point.y})`}
                className={`gv-node${clusterClass}${active ? ' gv-node-active' : ''}${node.outlier ? ' gv-node-outlier' : ''}${node.no_semantic_signal ? ' gv-node-no-signal' : ''}`}
                onClick={(event) => {
                  event.stopPropagation();
                  if (!suppressClick.current) onOpenNote(node.id);
                }}
                onMouseEnter={() => setHoveredNodeId(node.id)}
                onMouseLeave={() => setHoveredNodeId((current) => current === node.id ? null : current)}
              >
                <NodeShape graphNode={themeStyle.graphNode} active={active} />
                {node.outlier && <circle r={12} className="gv-node-status-ring" />}
                {node.no_semantic_signal && <circle r={11} className="gv-node-status-ring" />}
                {(showAllTitles || active) && <text x={13} y={4} className="gv-node-label">{node.title}</text>}
              </g>
            );
          })}
        </g>
      </svg>

      <div className="gv-camera-controls" aria-label="Graph zoom controls">
        <button type="button" title="Zoom in" aria-label="Zoom in" onClick={() => zoomBy(1.3)}><Icon name="add" size={18} /></button>
        <button type="button" title="Zoom out" aria-label="Zoom out" onClick={() => zoomBy(1 / 1.3)}><Icon name="remove" size={18} /></button>
        <button type="button" title="Fit graph" aria-label="Fit graph" onClick={() => setCamera(INITIAL_CAMERA)}><Icon name="fit_screen" size={18} /></button>
      </div>

      <div className="gv-legend">
        <span><i className="gv-legend-line semantic" />Semantic similarity</span>
        {showPriorRelationships && <span><i className="gv-legend-line prior" />Prior relationship</span>}
        <span><i className="gv-legend-region" />Cluster</span>
        <span><i className="gv-legend-node outlier" />Outlier</span>
        <span><i className="gv-legend-node no-signal" />No semantic signal</span>
      </div>
      {loading && <div className="gv-loading-indicator">Reweaving graph…</div>}
    </div>
  );
}

function NodeShape({ graphNode, active }: { graphNode: string; active: boolean }) {
  const radius = active ? 8 : 6;
  if (graphNode === 'star') {
    const inset = radius * 0.42;
    const path = `M0,${-radius} L${inset},${-inset} L${radius},0 L${inset},${inset} L0,${radius} L${-inset},${inset} L${-radius},0 L${-inset},${-inset} Z`;
    return <path d={path} className="gv-node-dot" />;
  }
  if (graphNode === 'hex') {
    const points = Array.from({ length: 6 }, (_, index) => {
      const angle = Math.PI / 3 * index;
      return `${(radius * Math.cos(angle)).toFixed(2)},${(radius * Math.sin(angle)).toFixed(2)}`;
    }).join(' ');
    return <polygon points={points} className="gv-node-dot" />;
  }
  return <circle r={radius} className="gv-node-dot" />;
}

function pointerDistance(points: GraphPoint[]): number {
  return Math.hypot(points[0].x - points[1].x, points[0].y - points[1].y);
}

function regionLabelPoint(points: GraphPoint[]): GraphPoint {
  return {
    x: points.reduce((sum, point) => sum + point.x, 0) / points.length,
    y: Math.min(...points.map((point) => point.y)) - 34,
  };
}
