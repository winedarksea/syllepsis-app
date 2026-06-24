import type { GraphAnalysisNode, GraphSemanticEdge } from '../types';

export const GRAPH_WIDTH = 1000;
export const GRAPH_HEIGHT = 720;
export const GRAPH_PADDING_X = 70;
export const GRAPH_PADDING_Y = 54;

export interface GraphPoint {
  x: number;
  y: number;
}

export interface GraphCamera {
  x: number;
  y: number;
  zoom: number;
}

export function graphNodePoint(node: GraphAnalysisNode): GraphPoint {
  return {
    x: GRAPH_PADDING_X + node.x * (GRAPH_WIDTH - GRAPH_PADDING_X * 2),
    y: GRAPH_PADDING_Y + node.y * (GRAPH_HEIGHT - GRAPH_PADDING_Y * 2),
  };
}

export function filterSemanticEdges(
  edges: GraphSemanticEdge[],
  threshold: number,
): GraphSemanticEdge[] {
  return edges.filter((edge) => edge.similarity >= threshold);
}

export function zoomCameraAtPoint(
  camera: GraphCamera,
  point: GraphPoint,
  requestedZoom: number,
): GraphCamera {
  const zoom = Math.min(8, Math.max(0.5, requestedZoom));
  const ratio = camera.zoom / zoom;
  return {
    x: point.x - (point.x - camera.x) * ratio,
    y: point.y - (point.y - camera.y) * ratio,
    zoom,
  };
}

function cross(origin: GraphPoint, a: GraphPoint, b: GraphPoint): number {
  return (a.x - origin.x) * (b.y - origin.y) - (a.y - origin.y) * (b.x - origin.x);
}

export function convexHull(points: GraphPoint[]): GraphPoint[] {
  if (points.length <= 2) return [...points];
  const sorted = [...points].sort((a, b) => a.x - b.x || a.y - b.y);
  const lower: GraphPoint[] = [];
  for (const point of sorted) {
    while (lower.length >= 2 && cross(lower.at(-2)!, lower.at(-1)!, point) <= 0) lower.pop();
    lower.push(point);
  }
  const upper: GraphPoint[] = [];
  for (const point of sorted.reverse()) {
    while (upper.length >= 2 && cross(upper.at(-2)!, upper.at(-1)!, point) <= 0) upper.pop();
    upper.push(point);
  }
  lower.pop();
  upper.pop();
  return [...lower, ...upper];
}

export function paddedHullPath(points: GraphPoint[], padding = 30): string {
  const hull = convexHull(points);
  if (hull.length < 3) return '';
  const center = hull.reduce(
    (sum, point) => ({ x: sum.x + point.x / hull.length, y: sum.y + point.y / hull.length }),
    { x: 0, y: 0 },
  );
  const padded = hull.map((point) => {
    const dx = point.x - center.x;
    const dy = point.y - center.y;
    const length = Math.hypot(dx, dy) || 1;
    return { x: point.x + dx / length * padding, y: point.y + dy / length * padding };
  });
  return `${padded.map((point, index) => `${index === 0 ? 'M' : 'L'}${point.x},${point.y}`).join(' ')} Z`;
}
