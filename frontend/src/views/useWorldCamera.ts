import { useCallback, useState } from 'react';
import type { PointerEvent as ReactPointerEvent, WheelEvent as ReactWheelEvent } from 'react';

export interface WorldCamera {
  x: number;
  y: number;
  zoom: number;
}

export function useWorldCamera(width: number, height: number) {
  const [camera, setCamera] = useState<WorldCamera>({ x: 0, y: 0, zoom: 1 });
  const [dragOrigin, setDragOrigin] = useState<{ clientX: number; clientY: number; camera: WorldCamera } | null>(null);

  const clampCamera = useCallback((candidate: WorldCamera): WorldCamera => {
    const viewportWidth = width / candidate.zoom;
    const viewportHeight = height / candidate.zoom;
    return {
      zoom: candidate.zoom,
      x: Math.max(0, Math.min(width - viewportWidth, candidate.x)),
      y: Math.max(0, Math.min(height - viewportHeight, candidate.y)),
    };
  }, [height, width]);

  const reset = useCallback(() => setCamera({ x: 0, y: 0, zoom: 1 }), []);

  const onWheel = useCallback((event: ReactWheelEvent<SVGSVGElement>) => {
    event.preventDefault();
    const rectangle = event.currentTarget.getBoundingClientRect();
    const fractionX = (event.clientX - rectangle.left) / rectangle.width;
    const fractionY = (event.clientY - rectangle.top) / rectangle.height;
    setCamera((current) => {
      const nextZoom = Math.max(1, Math.min(12, current.zoom * Math.exp(-event.deltaY * 0.0015)));
      const worldX = current.x + fractionX * width / current.zoom;
      const worldY = current.y + fractionY * height / current.zoom;
      return clampCamera({
        zoom: nextZoom,
        x: worldX - fractionX * width / nextZoom,
        y: worldY - fractionY * height / nextZoom,
      });
    });
  }, [clampCamera, height, width]);

  const onPointerDown = useCallback((event: ReactPointerEvent<SVGSVGElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId);
    setDragOrigin({ clientX: event.clientX, clientY: event.clientY, camera });
  }, [camera]);

  const onPointerMove = useCallback((event: ReactPointerEvent<SVGSVGElement>) => {
    if (!dragOrigin) return;
    const rectangle = event.currentTarget.getBoundingClientRect();
    const deltaX = (event.clientX - dragOrigin.clientX) * width / rectangle.width / dragOrigin.camera.zoom;
    const deltaY = (event.clientY - dragOrigin.clientY) * height / rectangle.height / dragOrigin.camera.zoom;
    setCamera(clampCamera({
      ...dragOrigin.camera,
      x: dragOrigin.camera.x - deltaX,
      y: dragOrigin.camera.y - deltaY,
    }));
  }, [clampCamera, dragOrigin, height, width]);

  const onPointerUp = useCallback((event: ReactPointerEvent<SVGSVGElement>) => {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    setDragOrigin(null);
  }, []);

  return { camera, reset, onWheel, onPointerDown, onPointerMove, onPointerUp };
}
