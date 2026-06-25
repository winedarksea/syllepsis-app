import { useRef, useState } from 'react';
import type { PointerEvent as ReactPointerEvent, WheelEvent as ReactWheelEvent, RefObject } from 'react';
import type { GraphPoint } from './graphGeometry';

// A camera with independent horizontal/vertical zoom. GraphCanvas keeps its own uniform camera;
// this hook powers TimelineCanvas, where the time axis (x) zooms separately from the stack axis (y).
export interface Camera2D {
  x: number;
  y: number;
  zoomX: number;
  zoomY: number;
}

interface UseGraphCameraOptions {
  width: number;
  height: number;
  initial: Camera2D;
  minZoom?: number;
  maxZoom?: number;
}

const clampZoom = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

function zoomAt(camera: Camera2D, point: GraphPoint, zoomX: number, zoomY: number): Camera2D {
  return {
    zoomX,
    zoomY,
    x: point.x - (point.x - camera.x) * (camera.zoomX / zoomX),
    y: point.y - (point.y - camera.y) * (camera.zoomY / zoomY),
  };
}

function pointerDistance(points: GraphPoint[]): number {
  return Math.hypot(points[0].x - points[1].x, points[0].y - points[1].y);
}

export function useGraphCamera(svgRef: RefObject<SVGSVGElement | null>, options: UseGraphCameraOptions) {
  const { width, height, initial } = options;
  const minZoom = options.minZoom ?? 0.25;
  const maxZoom = options.maxZoom ?? 40;
  const [camera, setCamera] = useState<Camera2D>(initial);
  const activePointers = useRef(new Map<number, GraphPoint>());
  const dragState = useRef<{ pointerId: number; x: number; y: number } | null>(null);
  const pinchState = useRef<{ distance: number } | null>(null);
  const suppressClick = useRef(false);

  const clientToWorld = (clientX: number, clientY: number): GraphPoint => {
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect) return { x: camera.x, y: camera.y };
    return {
      x: camera.x + (clientX - rect.left) / rect.width * (width / camera.zoomX),
      y: camera.y + (clientY - rect.top) / rect.height * (height / camera.zoomY),
    };
  };

  const onWheel = (event: ReactWheelEvent<SVGSVGElement>) => {
    event.preventDefault();
    const factor = Math.exp(-event.deltaY * 0.0015);
    const point = clientToWorld(event.clientX, event.clientY);
    setCamera((current) => (event.shiftKey
      ? zoomAt(current, point, current.zoomX, clampZoom(current.zoomY * factor, minZoom, maxZoom))
      : zoomAt(current, point, clampZoom(current.zoomX * factor, minZoom, maxZoom), current.zoomY)));
  };

  const onPointerDown = (event: ReactPointerEvent<SVGSVGElement>) => {
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

  const onPointerMove = (event: ReactPointerEvent<SVGSVGElement>) => {
    if (!activePointers.current.has(event.pointerId)) return;
    activePointers.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
    if (activePointers.current.size === 2) {
      const points = [...activePointers.current.values()];
      const distance = pointerDistance(points);
      const midpoint = { x: (points[0].x + points[1].x) / 2, y: (points[0].y + points[1].y) / 2 };
      const previousDistance = pinchState.current?.distance ?? distance;
      const worldMidpoint = clientToWorld(midpoint.x, midpoint.y);
      const ratio = distance / previousDistance;
      setCamera((current) => zoomAt(
        current,
        worldMidpoint,
        clampZoom(current.zoomX * ratio, minZoom, maxZoom),
        clampZoom(current.zoomY * ratio, minZoom, maxZoom),
      ));
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
      x: current.x - dx / rect.width * (width / current.zoomX),
      y: current.y - dy / rect.height * (height / current.zoomY),
    }));
    dragState.current = { pointerId: event.pointerId, x: event.clientX, y: event.clientY };
  };

  const onPointerEnd = (event: ReactPointerEvent<SVGSVGElement>) => {
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
      x: camera.x + width / camera.zoomX / 2,
      y: camera.y + height / camera.zoomY / 2,
    };
    setCamera((current) => zoomAt(
      current,
      center,
      clampZoom(current.zoomX * factor, minZoom, maxZoom),
      clampZoom(current.zoomY * factor, minZoom, maxZoom),
    ));
  };

  const viewBox = `${camera.x} ${camera.y} ${width / camera.zoomX} ${height / camera.zoomY}`;

  return {
    camera,
    setCamera,
    viewBox,
    suppressClick,
    zoomBy,
    handlers: { onWheel, onPointerDown, onPointerMove, onPointerUp: onPointerEnd, onPointerCancel: onPointerEnd },
  };
}
