import { useEffect, useMemo, useRef, useState } from 'react';
import { Icon } from '../components/Icon';
import type { GraphAnalysisResult, GraphTimelineTick, TimelineColorBy } from '../types';
import {
  GRAPH_HEIGHT, GRAPH_PADDING_X, GRAPH_WIDTH,
} from './graphGeometry';
import { useGraphCamera, type Camera2D } from './useGraphCamera';

const MIN_ZOOM = 0.25;
const MAX_ZOOM = 40;

// Axis sits 48px from the SVG bottom; nodes fill the space above it with a small top margin.
const TL_AXIS_WORLD_Y = GRAPH_HEIGHT - 48;
const TL_STACK_TOP = 20;
const TL_STACK_BOTTOM = TL_AXIS_WORLD_Y - 20;

// Normalized node.y [0,1] → world pixel Y (timeline-specific, no symmetric GRAPH_PADDING_Y).
const tlWorldY = (ny: number) => TL_STACK_TOP + ny * (TL_STACK_BOTTOM - TL_STACK_TOP);

interface TimelineCanvasProps {
  result: GraphAnalysisResult;
  showAllTitles: boolean;
  colorBy: TimelineColorBy;
  loading: boolean;
  onOpenNote: (id: string) => void;
}

// Normalized [0,1] node-x → world pixel (same mapping graphNodePoint uses for x).
const nxToWorld = (nx: number) => GRAPH_PADDING_X + nx * (GRAPH_WIDTH - GRAPH_PADDING_X * 2);

export function TimelineCanvas({
  result,
  showAllTitles,
  colorBy,
  loading,
  onOpenNote,
}: TimelineCanvasProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [hoveredNodeId, setHoveredNodeId] = useState<string | null>(null);
  const timeline = result.timeline;

  // Initial camera frames the focus (≈p1–p99) window horizontally and the full height vertically.
  const initialCamera = useMemo<Camera2D>(() => {
    const start = timeline?.focus_start_x ?? 0;
    const end = timeline?.focus_end_x ?? 1;
    const worldStart = nxToWorld(start);
    const worldWidth = Math.max(1, nxToWorld(end) - worldStart);
    const zoomX = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, GRAPH_WIDTH / worldWidth));
    return { x: worldStart, y: 0, zoomX, zoomY: 1 };
  }, [timeline?.focus_start_x, timeline?.focus_end_x]);

  const cam = useGraphCamera(svgRef, {
    width: GRAPH_WIDTH,
    height: GRAPH_HEIGHT,
    initial: initialCamera,
    minZoom: MIN_ZOOM,
    maxZoom: MAX_ZOOM,
  });
  const { camera, setCamera } = cam;

  // Re-frame when the focus window changes (date field / granularity / corpus change).
  useEffect(() => { setCamera(initialCamera); }, [initialCamera, setCamera]);

  const projectX = (worldX: number) => (worldX - camera.x) * camera.zoomX;
  const projectY = (worldY: number) => (worldY - camera.y) * camera.zoomY;

  const pointsById = useMemo(
    () => new Map(result.nodes.map((node) => [
      node.id,
      { x: nxToWorld(node.x), y: tlWorldY(node.y) },
    ])),
    [result.nodes],
  );

  // Axis Y in screen/viewBox space — moves with Y zoom so nodes and axis stay aligned.
  const axisScreenY = projectY(TL_AXIS_WORLD_Y);

  // Only show ticks that are at least 60px apart in screen space; recomputes on camera change.
  const visibleTicks = useMemo(() => {
    if (!timeline?.ticks?.length) return [] as GraphTimelineTick[];
    const result: GraphTimelineTick[] = [];
    let lastX = -Infinity;
    for (const tick of timeline.ticks) {
      const x = (nxToWorld(tick.x) - camera.x) * camera.zoomX;
      if (x - lastX >= 60) {
        result.push(tick);
        lastX = x;
      }
    }
    return result;
  }, [timeline?.ticks, camera]);

  const undatedCount = timeline?.undated_count ?? 0;
  // Divider between the undated lane and the dated time axis (matches the backend lane layout).
  const dividerWorldX = nxToWorld(0.075);

  return (
    <div className={`gv-canvas${loading ? ' loading' : ''}`}>
      <svg
        ref={svgRef}
        viewBox={`0 0 ${GRAPH_WIDTH} ${GRAPH_HEIGHT}`}
        className="gv-svg tl-svg"
        {...cam.handlers}
      >
        <g className="tl-axis">
          {visibleTicks.map((tick) => {
            const x = projectX(nxToWorld(tick.x));
            return (
              <g key={tick.at_ms} className="tl-tick">
                <line x1={x} y1={0} x2={x} y2={axisScreenY} className="tl-gridline" />
                <text x={x} y={axisScreenY + 15} className="tl-tick-label">{tick.label}</text>
              </g>
            );
          })}
          <line x1={0} y1={axisScreenY} x2={GRAPH_WIDTH} y2={axisScreenY} className="tl-axis-line" />
        </g>

        {undatedCount > 0 && (
          <g className="tl-undated">
            <line x1={projectX(dividerWorldX)} y1={0} x2={projectX(dividerWorldX)} y2={axisScreenY} className="tl-undated-divider" />
            <text x={projectX(nxToWorld(0.025))} y={18} className="tl-undated-label">{`Undated (${undatedCount})`}</text>
          </g>
        )}

        <g className="gv-prior-edges">
          {result.prior_edges.map((edge) => {
            const source = pointsById.get(edge.source);
            const target = pointsById.get(edge.target);
            if (!source || !target) return null;
            return (
              <line
                key={`${edge.source}:${edge.target}`}
                x1={projectX(source.x)}
                y1={projectY(source.y)}
                x2={projectX(target.x)}
                y2={projectY(target.y)}
                className="gv-prior-edge"
              />
            );
          })}
        </g>

        <g className="gv-nodes">
          {result.nodes.map((node) => {
            const world = pointsById.get(node.id)!;
            const x = projectX(world.x);
            const y = projectY(world.y);
            const active = hoveredNodeId === node.id;
            const clusterClass = node.cluster_id === undefined ? '' : ` gv-node--${node.cluster_id % 5}`;
            return (
              <g
                key={node.id}
                transform={`translate(${x} ${y})`}
                className={`gv-node${clusterClass}${active ? ' gv-node-active' : ''}${node.no_semantic_signal ? ' gv-node-no-signal' : ''}`}
                onClick={(event) => {
                  event.stopPropagation();
                  if (!cam.suppressClick.current) onOpenNote(node.id);
                }}
                onMouseEnter={() => setHoveredNodeId(node.id)}
                onMouseLeave={() => setHoveredNodeId((current) => current === node.id ? null : current)}
              >
                <circle r={active ? 8 : 6} className="gv-node-dot" />
                {node.no_semantic_signal && <circle r={11} className="gv-node-status-ring" />}
                {(showAllTitles || active) && <text x={13} y={4} className="gv-node-label">{node.title}</text>}
              </g>
            );
          })}
        </g>
      </svg>

      <div className="gv-camera-controls" aria-label="Timeline zoom controls">
        <button type="button" title="Zoom in" aria-label="Zoom in" onClick={() => cam.zoomBy(1.3)}><Icon name="add" size={18} /></button>
        <button type="button" title="Zoom out" aria-label="Zoom out" onClick={() => cam.zoomBy(1 / 1.3)}><Icon name="remove" size={18} /></button>
        <button type="button" title="Fit timeline" aria-label="Fit timeline" onClick={() => setCamera(initialCamera)}><Icon name="fit_screen" size={18} /></button>
      </div>

      <div className="gv-legend">
        <span>Time →</span>
        <span><i className="gv-legend-line prior" />Prior relationship</span>
        <span>{`Colored by ${colorBy}`}</span>
        <span>Wheel: zoom time · Shift+wheel: zoom stacks</span>
      </div>
      {loading && <div className="gv-loading-indicator">Reweaving timeline…</div>}
    </div>
  );
}
