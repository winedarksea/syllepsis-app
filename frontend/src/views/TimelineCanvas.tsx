import { useEffect, useMemo, useRef, useState } from 'react';
import type { PointerEvent as ReactPointerEvent } from 'react';
import { Icon } from '../components/Icon';
import type {
  GraphAnalysisNode, GraphAnalysisResult, GraphTimelineTick, TimelineColorBy,
} from '../types';
import {
  GRAPH_HEIGHT, GRAPH_PADDING_X, GRAPH_WIDTH,
} from './graphGeometry';
import {
  formatTimelineDateSource, formatTimelineNodeDate,
} from './timelinePresentation';
import { useGraphCamera, type Camera2D } from './useGraphCamera';
import {
  findNearestActivatablePoint, SvgActivationTracker, svgClientPoint, svgUserPointToClient,
} from './graphInteraction';

const MIN_ZOOM = 0.25;
const MAX_ZOOM = 40;
const TIMELINE_AXIS_SCREEN_Y = GRAPH_HEIGHT - 82;
const TIMELINE_STACK_TOP = 24;
const TIMELINE_STACK_BOTTOM = TIMELINE_AXIS_SCREEN_Y - 24;
const TIMELINE_TOOLTIP_WIDTH = 250;
const TIMELINE_TOOLTIP_HEIGHT = 96;
const TIMELINE_RANGE_BAR_HEIGHT = 8;
const TIMELINE_RANGE_HIT_RADIUS = 12;

interface TimelineCanvasProps {
  result: GraphAnalysisResult;
  showAllTitles: boolean;
  showPriorRelationships: boolean;
  colorBy: TimelineColorBy;
  loading: boolean;
  onOpenNote: (id: string) => void;
}

const normalizedXToWorld = (normalizedX: number) =>
  GRAPH_PADDING_X + normalizedX * (GRAPH_WIDTH - GRAPH_PADDING_X * 2);

const normalizedYToWorld = (normalizedY: number) =>
  TIMELINE_STACK_TOP + normalizedY * (TIMELINE_STACK_BOTTOM - TIMELINE_STACK_TOP);

export function TimelineCanvas({
  result,
  showAllTitles,
  showPriorRelationships,
  colorBy,
  loading,
  onOpenNote,
}: TimelineCanvasProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const activationTracker = useRef(new SvgActivationTracker());
  const [hoveredNodeId, setHoveredNodeId] = useState<string | null>(null);
  const timeline = result.timeline;

  const initialCamera = useMemo<Camera2D>(() => {
    const start = timeline?.focus_start_x ?? 0;
    const end = timeline?.focus_end_x ?? 1;
    const worldStart = normalizedXToWorld(start);
    const worldWidth = Math.max(1, normalizedXToWorld(end) - worldStart);
    const zoomFromFocus = GRAPH_WIDTH / worldWidth;
    const zoomX = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoomFromFocus));

    return { x: worldStart, y: 0, zoomX, zoomY: 1 };
  }, [timeline?.focus_start_x, timeline?.focus_end_x]);

  const cameraController = useGraphCamera(svgRef, {
    width: GRAPH_WIDTH,
    height: GRAPH_HEIGHT,
    initial: initialCamera,
    minZoom: MIN_ZOOM,
    maxZoom: MAX_ZOOM,
  });
  const { camera, setCamera } = cameraController;

  useEffect(() => {
    setCamera(initialCamera);
  }, [initialCamera, setCamera]);

  const projectX = (worldX: number) => (worldX - camera.x) * camera.zoomX;
  const projectY = (worldY: number) => (worldY - camera.y) * camera.zoomY;

  const pointsById = useMemo(
    () => new Map(result.nodes.map((node) => [
      node.id,
      { x: normalizedXToWorld(node.x), y: normalizedYToWorld(node.y) },
    ])),
    [result.nodes],
  );

  const findNearestTimelineNode = (clientX: number, clientY: number): string | null => {
    const svg = svgRef.current;
    const screenPoint = { x: clientX, y: clientY };
    if (!svg) return null;
    const svgPoint = svgClientPoint(svg, clientX, clientY);
    if (svgPoint) {
      let nearestRangeId: string | null = null;
      let nearestRangeDistance = TIMELINE_RANGE_HIT_RADIUS;
      for (const node of result.nodes) {
        if (!node.timeline_range) continue;
        const startPoint = pointsById.get(node.id);
        if (!startPoint) continue;
        const startX = projectX(startPoint.x);
        const endX = projectX(normalizedXToWorld(node.timeline_range.end_x));
        const y = projectY(startPoint.y);
        const left = Math.min(startX, endX);
        const right = Math.max(startX, endX);
        if (svgPoint.x < left - TIMELINE_RANGE_HIT_RADIUS || svgPoint.x > right + TIMELINE_RANGE_HIT_RADIUS) {
          continue;
        }
        const distance = Math.abs(svgPoint.y - y);
        if (distance <= nearestRangeDistance) {
          nearestRangeId = node.id;
          nearestRangeDistance = distance;
        }
      }
      if (nearestRangeId) return nearestRangeId;
    }
    const rect = svg.getBoundingClientRect();
    const points = result.nodes.map((node) => {
      const worldPoint = pointsById.get(node.id)!;
      const x = projectX(worldPoint.x);
      const y = projectY(worldPoint.y);
      const clientPoint = svgUserPointToClient(svg, x, y);
      if (clientPoint) return { id: node.id, ...clientPoint };
      return {
        id: node.id,
        x: rect.left + x / GRAPH_WIDTH * rect.width,
        y: rect.top + y / GRAPH_HEIGHT * rect.height,
      };
    });
    return findNearestActivatablePoint(points, screenPoint);
  };

  const handlePointerDown = (event: ReactPointerEvent<SVGSVGElement>) => {
    cameraController.handlers.onPointerDown(event);
    activationTracker.current.pointerDown(event);
  };

  const handlePointerMove = (event: ReactPointerEvent<SVGSVGElement>) => {
    cameraController.handlers.onPointerMove(event);
    activationTracker.current.pointerMove(event);
  };

  const handlePointerEnd = (event: ReactPointerEvent<SVGSVGElement>) => {
    const activationPoint = activationTracker.current.pointerUp(event);
    if (activationPoint && !cameraController.suppressClick.current) {
      const activatedNodeId = findNearestTimelineNode(event.clientX, event.clientY);
      if (activatedNodeId) onOpenNote(activatedNodeId);
    }
    cameraController.handlers.onPointerUp(event);
  };

  const handlePointerCancel = (event: ReactPointerEvent<SVGSVGElement>) => {
    activationTracker.current.pointerCancel(event.pointerId);
    cameraController.handlers.onPointerCancel(event);
  };

  const visibleTicks = useMemo(() => {
    if (!timeline?.ticks?.length) return [] as GraphTimelineTick[];
    const ticks: GraphTimelineTick[] = [];
    let lastScreenX = -Infinity;
    for (const tick of timeline.ticks) {
      const screenX = (normalizedXToWorld(tick.x) - camera.x) * camera.zoomX;
      if (screenX < -30 || screenX > GRAPH_WIDTH + 30) continue;
      if (screenX - lastScreenX >= 72) {
        ticks.push(tick);
        lastScreenX = screenX;
      }
    }
    return ticks;
  }, [timeline, camera.x, camera.zoomX]);

  const hoveredNode = hoveredNodeId === null
    ? undefined
    : result.nodes.find((node) => node.id === hoveredNodeId);
  const hoveredPoint = hoveredNode ? pointsById.get(hoveredNode.id) : undefined;
  const undatedCount = timeline?.undated_count ?? 0;
  const dividerWorldX = normalizedXToWorld(0.075);
  const rangeMode = result.nodes.some((node) => node.timeline_range);

  return (
    <div className={`gv-canvas tl-canvas${loading ? ' loading' : ''}`}>
      <svg
        ref={svgRef}
        viewBox={`0 0 ${GRAPH_WIDTH} ${GRAPH_HEIGHT}`}
        className="gv-svg tl-svg"
        onWheel={cameraController.handlers.onWheel}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerEnd}
        onPointerCancel={handlePointerCancel}
      >
        <defs>
          <clipPath id="timeline-plot-clip">
            <rect x="0" y="0" width={GRAPH_WIDTH} height={TIMELINE_AXIS_SCREEN_Y - 7} />
          </clipPath>
        </defs>

        <g className="tl-grid">
          {visibleTicks.map((tick) => {
            const x = projectX(normalizedXToWorld(tick.x));
            return (
              <line
                key={tick.at_ms}
                x1={x}
                y1={0}
                x2={x}
                y2={TIMELINE_AXIS_SCREEN_Y}
                className="tl-gridline"
              />
            );
          })}
        </g>

        <g clipPath="url(#timeline-plot-clip)">
          {undatedCount > 0 && (
            <line
              x1={projectX(dividerWorldX)}
              y1={0}
              x2={projectX(dividerWorldX)}
              y2={TIMELINE_AXIS_SCREEN_Y}
              className="tl-undated-divider"
            />
          )}

          <g className="tl-stems">
            {result.nodes.map((node) => {
              if (!node.timeline_date) return null;
              const point = pointsById.get(node.id)!;
              const x = projectX(point.x);
              return (
                <line
                  key={node.id}
                  x1={x}
                  y1={projectY(point.y)}
                  x2={x}
                  y2={TIMELINE_AXIS_SCREEN_Y}
                  className="tl-stem"
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
          )}

          <g className="tl-ranges">
            {result.nodes.map((node) => {
              if (!node.timeline_date || !node.timeline_range) return null;
              const startPoint = pointsById.get(node.id)!;
              const startX = projectX(startPoint.x);
              const endX = projectX(normalizedXToWorld(node.timeline_range.end_x));
              const y = projectY(startPoint.y);
              const left = Math.min(startX, endX);
              const width = Math.max(2, Math.abs(endX - startX));
              const active = hoveredNodeId === node.id;
              const clusterClass = node.cluster_id === undefined
                ? ''
                : ` gv-node--${node.cluster_id % 5}`;
              return (
                <g
                  key={node.id}
                  className={`tl-range${clusterClass}${active ? ' active' : ''}${node.timeline_range.end_before_start ? ' reversed' : ''}`}
                  onMouseEnter={() => setHoveredNodeId(node.id)}
                  onMouseLeave={() =>
                    setHoveredNodeId((current) => current === node.id ? null : current)}
                >
                  <rect
                    x={left}
                    y={y - TIMELINE_RANGE_BAR_HEIGHT / 2}
                    width={width}
                    height={TIMELINE_RANGE_BAR_HEIGHT}
                    rx={TIMELINE_RANGE_BAR_HEIGHT / 2}
                    className="tl-range-bar"
                  />
                  <line
                    x1={endX}
                    y1={y - 6}
                    x2={endX}
                    y2={y + 6}
                    className="tl-range-end-cap"
                  />
                </g>
              );
            })}
          </g>

          <g className="gv-nodes">
            {result.nodes.map((node) => {
              const worldPoint = pointsById.get(node.id)!;
              const x = projectX(worldPoint.x);
              const y = projectY(worldPoint.y);
              const active = hoveredNodeId === node.id;
              const clusterClass = node.cluster_id === undefined
                ? ''
                : ` gv-node--${node.cluster_id % 5}`;
              return (
                <g
                  key={node.id}
                  transform={`translate(${x} ${y})`}
                  className={`gv-node${clusterClass}${active ? ' gv-node-active' : ''}${node.no_semantic_signal ? ' gv-node-no-signal' : ''}`}
                  onMouseEnter={() => setHoveredNodeId(node.id)}
                  onMouseLeave={() =>
                    setHoveredNodeId((current) => current === node.id ? null : current)}
                >
                  <circle r={active ? 8 : 6} className="gv-node-dot" />
                  {node.no_semantic_signal && <circle r={11} className="gv-node-status-ring" />}
                  {(showAllTitles || active) && (
                    <text x={13} y={4} className="gv-node-label">{node.title}</text>
                  )}
                </g>
              );
            })}
          </g>
        </g>

        <TimelineAxis visibleTicks={visibleTicks} projectX={projectX} />

        {undatedCount > 0 && (
          <text
            x={projectX(normalizedXToWorld(0.025))}
            y={18}
            className="tl-undated-label"
          >
            {`Undated (${undatedCount})`}
          </text>
        )}

        {hoveredNode && hoveredPoint && (
          <TimelineHoverCard
            node={hoveredNode}
            x={projectX(hoveredPoint.x)}
            y={projectY(hoveredPoint.y)}
          />
        )}
      </svg>

      <div className="gv-camera-controls" aria-label="Timeline zoom controls">
        <button
          type="button"
          title="Zoom in"
          aria-label="Zoom in"
          onClick={() => cameraController.zoomBy(1.3)}
        >
          <Icon name="add" size={18} />
        </button>
        <button
          type="button"
          title="Zoom out"
          aria-label="Zoom out"
          onClick={() => cameraController.zoomBy(1 / 1.3)}
        >
          <Icon name="remove" size={18} />
        </button>
        <button
          type="button"
          title="Fit timeline"
          aria-label="Fit timeline"
          onClick={() => setCamera(initialCamera)}
        >
          <Icon name="fit_screen" size={18} />
        </button>
      </div>

      <div className="gv-legend">
        <span><i className="tl-legend-stem" />{rangeMode ? 'Date range' : 'Date'}</span>
        {showPriorRelationships && (
          <span><i className="gv-legend-line prior" />Prior relationship</span>
        )}
        <span>{`Colored by ${colorBy}`}</span>
        <span>Wheel: zoom time · Shift+wheel: zoom stacks</span>
      </div>
      {loading && <div className="gv-loading-indicator">Reweaving timeline…</div>}
    </div>
  );
}

interface TimelineAxisProps {
  visibleTicks: GraphTimelineTick[];
  projectX: (worldX: number) => number;
}

function TimelineAxis({ visibleTicks, projectX }: TimelineAxisProps) {
  return (
    <g className="tl-axis">
      <line
        x1={0}
        y1={TIMELINE_AXIS_SCREEN_Y}
        x2={GRAPH_WIDTH}
        y2={TIMELINE_AXIS_SCREEN_Y}
        className="tl-axis-line"
      />
      <text x={12} y={TIMELINE_AXIS_SCREEN_Y - 10} className="tl-axis-title">Time</text>
      {visibleTicks.map((tick) => {
        const x = projectX(normalizedXToWorld(tick.x));
        return (
          <g key={tick.at_ms} className="tl-tick">
            <line
              x1={x}
              y1={TIMELINE_AXIS_SCREEN_Y - 5}
              x2={x}
              y2={TIMELINE_AXIS_SCREEN_Y + 6}
              className="tl-tick-mark"
            />
            <text x={x} y={TIMELINE_AXIS_SCREEN_Y + 22} className="tl-tick-label">
              {tick.label}
            </text>
          </g>
        );
      })}
    </g>
  );
}

function TimelineHoverCard({ node, x, y }: {
  node: GraphAnalysisNode;
  x: number;
  y: number;
}) {
  const cardX = x + 16 + TIMELINE_TOOLTIP_WIDTH > GRAPH_WIDTH
    ? x - TIMELINE_TOOLTIP_WIDTH - 16
    : x + 16;
  const cardY = Math.min(
    TIMELINE_AXIS_SCREEN_Y - TIMELINE_TOOLTIP_HEIGHT - 12,
    Math.max(12, y - TIMELINE_TOOLTIP_HEIGHT / 2),
  );
  const title = node.title.length > 34 ? `${node.title.slice(0, 33)}…` : node.title;
  const hasRange = Boolean(node.timeline_range);
  const dateText = node.timeline_date
    ? `${hasRange ? 'Start ' : ''}${formatTimelineNodeDate(node.timeline_date)}`
    : 'No resolvable date';
  const sourceText = node.timeline_date
    ? formatTimelineDateSource(node.timeline_date)
    : 'Undated';
  const endText = node.timeline_range
    ? `End ${formatTimelineNodeDate(node.timeline_range.end_date)}`
    : null;
  const rangeWarning = node.timeline_range?.end_before_start ? 'End is before start' : sourceText;

  return (
    <g
      className="tl-hover-card"
      transform={`translate(${Math.max(8, cardX)} ${cardY})`}
      pointerEvents="none"
    >
      <rect width={TIMELINE_TOOLTIP_WIDTH} height={TIMELINE_TOOLTIP_HEIGHT} rx={8} />
      <text x={12} y={22} className="tl-hover-title">{title}</text>
      <text x={12} y={45} className="tl-hover-date">{dateText}</text>
      {endText && <text x={12} y={63} className="tl-hover-date">{endText}</text>}
      <text x={12} y={endText ? 82 : 63} className="tl-hover-source">{rangeWarning}</text>
    </g>
  );
}
