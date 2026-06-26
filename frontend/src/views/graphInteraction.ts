export interface GraphScreenPoint {
  x: number;
  y: number;
}

export interface GraphActivatablePoint extends GraphScreenPoint {
  id: string;
}

export const GRAPH_NODE_ACTIVATION_RADIUS_PX = 16;
export const GRAPH_DRAG_THRESHOLD_PX = 3;

export function findNearestActivatablePoint(
  points: GraphActivatablePoint[],
  target: GraphScreenPoint,
  radiusPx = GRAPH_NODE_ACTIVATION_RADIUS_PX,
): string | null {
  const radiusSquared = radiusPx * radiusPx;
  let nearestId: string | null = null;
  let nearestDistanceSquared = radiusSquared;

  for (const point of points) {
    const dx = point.x - target.x;
    const dy = point.y - target.y;
    const distanceSquared = dx * dx + dy * dy;
    if (distanceSquared <= nearestDistanceSquared) {
      nearestId = point.id;
      nearestDistanceSquared = distanceSquared;
    }
  }

  return nearestId;
}
