export interface GraphScreenPoint {
  x: number;
  y: number;
}

export interface GraphActivatablePoint extends GraphScreenPoint {
  id: string;
}

export const GRAPH_NODE_ACTIVATION_RADIUS_PX = 16;
export const GRAPH_DRAG_THRESHOLD_PX = 3;

export interface SvgActivationPointer {
  pointerId: number;
  clientX: number;
  clientY: number;
}

interface SvgActivationTrackerOptions {
  dragThresholdPx?: number;
}

// Tracks one pointer gesture and returns a release point only for intentional taps/clicks.
// Cameras can still pan, pinch, and capture pointers; activation is decided at pointer-up.
export class SvgActivationTracker {
  private activePointers = new Map<number, GraphScreenPoint>();
  private activationStart: ({ pointerId: number } & GraphScreenPoint) | null = null;
  private gestureSuppressed = false;
  private readonly dragThresholdPx: number;

  constructor(options: SvgActivationTrackerOptions = {}) {
    this.dragThresholdPx = options.dragThresholdPx ?? GRAPH_DRAG_THRESHOLD_PX;
  }

  pointerDown(pointer: SvgActivationPointer) {
    this.activePointers.set(pointer.pointerId, { x: pointer.clientX, y: pointer.clientY });
    if (this.activePointers.size === 1) {
      this.gestureSuppressed = false;
      this.activationStart = {
        pointerId: pointer.pointerId,
        x: pointer.clientX,
        y: pointer.clientY,
      };
    } else {
      this.gestureSuppressed = true;
      this.activationStart = null;
    }
  }

  pointerMove(pointer: SvgActivationPointer) {
    if (!this.activePointers.has(pointer.pointerId)) return;
    this.activePointers.set(pointer.pointerId, { x: pointer.clientX, y: pointer.clientY });
    const activationStart = this.activationStart;
    if (
      !activationStart
      || activationStart.pointerId !== pointer.pointerId
      || this.activePointers.size !== 1
    ) {
      this.gestureSuppressed = true;
      this.activationStart = null;
      return;
    }
    if (distance(pointerScreenPoint(pointer), activationStart) > this.dragThresholdPx) {
      this.gestureSuppressed = true;
      this.activationStart = null;
    }
  }

  pointerUp(pointer: SvgActivationPointer): GraphScreenPoint | null {
    const activationStart = this.activationStart;
    const activationPoint = activationStart
      && activationStart.pointerId === pointer.pointerId
      && !this.gestureSuppressed
      && this.activePointers.size === 1
      && distance(pointerScreenPoint(pointer), activationStart) <= this.dragThresholdPx
      ? { x: pointer.clientX, y: pointer.clientY }
      : null;

    this.activePointers.delete(pointer.pointerId);
    if (activationStart?.pointerId === pointer.pointerId) this.activationStart = null;
    if (this.activePointers.size === 0) this.gestureSuppressed = false;
    return activationPoint;
  }

  pointerCancel(pointerId: number) {
    this.activePointers.delete(pointerId);
    if (this.activationStart?.pointerId === pointerId) this.activationStart = null;
    this.gestureSuppressed = true;
    if (this.activePointers.size === 0) this.gestureSuppressed = false;
  }
}

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

function distance(a: GraphScreenPoint, b: GraphScreenPoint): number {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

function pointerScreenPoint(pointer: SvgActivationPointer): GraphScreenPoint {
  return { x: pointer.clientX, y: pointer.clientY };
}
