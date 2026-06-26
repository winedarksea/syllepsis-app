import { useRef, useState } from 'react';
import type {
  MouseEvent as ReactMouseEvent, PointerEvent as ReactPointerEvent,
} from 'react';
import type { Overlay, OverlayRegion, Pin, WorldPoint } from '../types';
import lowDetailEarthUrl from '../assets/earth/countries-equal-earth-low.svg';
import highDetailEarthUrl from '../assets/earth/countries-equal-earth-high.svg';
import { useWorldCamera, type WorldCamera } from './useWorldCamera';
import { WorldGrid } from './WorldGrid';
import { equalEarthInverseNormalized, equalEarthNormalized } from './worldProjection';
import {
  findNearestActivatablePoint, SvgActivationTracker, svgClientPoint, svgUserPointToClient,
} from './graphInteraction';

const STAGE_WIDTH = 1200;
const EARTH_HEIGHT = 620;
const WORLD_PIN_ACTIVATION_RADIUS_PX = 16;
const WORLD_REGION_MARKER_ACTIVATION_RADIUS_PX = 14;

interface Props {
  overlay: Overlay;
  backdrop: string | null;
  showGrid: boolean;
  onOpenNote: (id: string) => void;
  onOpenCategory: (name: string) => void;
}

export function WorldStage({
  overlay, backdrop, showGrid, onOpenNote, onOpenCategory,
}: Props) {
  const world = overlay.world;
  const height = world.kind === 'image' && world.intrinsic_dimensions
    ? STAGE_WIDTH * world.intrinsic_dimensions[1] / world.intrinsic_dimensions[0]
    : EARTH_HEIGHT;
  const cameraController = useWorldCamera(STAGE_WIDTH, height);
  const { camera } = cameraController;
  const activationTracker = useRef(new SvgActivationTracker());
  const [cursorLabel, setCursorLabel] = useState<string | null>(null);
  const viewBox = `${camera.x} ${camera.y} ${STAGE_WIDTH / camera.zoom} ${height / camera.zoom}`;
  const highDetailVisible = world.kind === 'geo' && camera.zoom >= 2.2;

  const clientToStagePoint = (
    event: Pick<ReactMouseEvent<SVGSVGElement> | ReactPointerEvent<SVGSVGElement>, 'currentTarget' | 'clientX' | 'clientY'>,
  ) => {
    const svgPoint = svgClientPoint(event.currentTarget, event.clientX, event.clientY);
    if (svgPoint) return svgPoint;
    const rectangle = event.currentTarget.getBoundingClientRect();
    return {
      x: camera.x + (event.clientX - rectangle.left) / rectangle.width * STAGE_WIDTH / camera.zoom,
      y: camera.y + (event.clientY - rectangle.top) / rectangle.height * height / camera.zoom,
    };
  };

  const stageToClientPoint = (
    stagePoint: { x: number; y: number },
    svg: SVGSVGElement,
  ) => ({
    ...(
      svgUserPointToClient(svg, stagePoint.x, stagePoint.y)
      ?? fallbackStageToClientPoint(stagePoint, svg.getBoundingClientRect(), camera, height)
    ),
  });

  const updateCursor = (event: ReactMouseEvent<SVGSVGElement>) => {
    const stagePoint = clientToStagePoint(event);
    const normalizedX = stagePoint.x / STAGE_WIDTH;
    const normalizedY = stagePoint.y / height;
    if (world.kind === 'image') {
      setCursorLabel(`x ${normalizedX.toFixed(3)}, y ${normalizedY.toFixed(3)}`);
    } else {
      const coordinate = equalEarthInverseNormalized(normalizedX, normalizedY);
      setCursorLabel(coordinate ? `${coordinate[1].toFixed(3)}°, ${coordinate[0].toFixed(3)}°` : null);
    }
  };

  const resolveActivationTarget = (event: ReactPointerEvent<SVGSVGElement>) => {
    const svg = event.currentTarget;
    const screenPoint = { x: event.clientX, y: event.clientY };
    const stagePoint = clientToStagePoint(event);

    const pinPoints = overlay.pins.map((pin, index) => ({
      id: String(index),
      ...stageToClientPoint(project(pin.point, STAGE_WIDTH, height), svg),
    }));
    const nearestPinIndex = findNearestActivatablePoint(
      pinPoints,
      screenPoint,
      WORLD_PIN_ACTIVATION_RADIUS_PX,
    );
    if (nearestPinIndex !== null) return overlay.pins[Number(nearestPinIndex)].target;

    const containingRegion = overlay.regions.find((region) =>
      isStagePointInsideRegion(region, stagePoint, STAGE_WIDTH, height));
    if (containingRegion) return { kind: 'category' as const, name: containingRegion.category };

    const regionMarkerPoints = overlay.regions.map((region, index) => ({
      id: String(index),
      ...stageToClientPoint(project(region.anchor, STAGE_WIDTH, height), svg),
    }));
    const nearestRegionIndex = findNearestActivatablePoint(
      regionMarkerPoints,
      screenPoint,
      WORLD_REGION_MARKER_ACTIVATION_RADIUS_PX,
    );
    if (nearestRegionIndex !== null) {
      return { kind: 'category' as const, name: overlay.regions[Number(nearestRegionIndex)].category };
    }
    return null;
  };

  const handlePointerDown = (event: ReactPointerEvent<SVGSVGElement>) => {
    cameraController.onPointerDown(event);
    activationTracker.current.pointerDown(event);
  };

  const handlePointerMove = (event: ReactPointerEvent<SVGSVGElement>) => {
    cameraController.onPointerMove(event);
    activationTracker.current.pointerMove(event);
  };

  const handlePointerUp = (event: ReactPointerEvent<SVGSVGElement>) => {
    const activationPoint = activationTracker.current.pointerUp(event);
    if (activationPoint) {
      const target = resolveActivationTarget(event);
      if (target?.kind === 'note') onOpenNote(target.id);
      if (target?.kind === 'category') onOpenCategory(target.name);
    }
    cameraController.onPointerUp(event);
  };

  const handlePointerCancel = (event: ReactPointerEvent<SVGSVGElement>) => {
    activationTracker.current.pointerCancel(event.pointerId);
    cameraController.onPointerUp(event);
  };

  return (
    <div className="wv-stage-wrap">
      <div className="wv-stage-toolbar">
        <span>{camera.zoom.toFixed(1)}×</span>
        <button onClick={cameraController.reset}>Fit world</button>
        {cursorLabel && <code>{cursorLabel}</code>}
      </div>
      <svg
        className="wv-svg-stage"
        viewBox={viewBox}
        onWheel={cameraController.onWheel}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerCancel}
        onMouseMove={updateCursor}
        onMouseLeave={() => setCursorLabel(null)}
        role="img"
        aria-label={world.display_name}
      >
        <rect className="wv-stage-background" width={STAGE_WIDTH} height={height} />
        {world.kind === 'geo' ? (
          <>
            <image className="wv-earth-land" href={lowDetailEarthUrl} width={STAGE_WIDTH} height={height} />
            {highDetailVisible && (
              <image className="wv-earth-land wv-earth-land-high" href={highDetailEarthUrl} width={STAGE_WIDTH} height={height} />
            )}
          </>
        ) : backdrop ? (
          <image href={backdrop} width={STAGE_WIDTH} height={height} preserveAspectRatio="none" />
        ) : (
          <text className="wv-missing-backdrop" x={STAGE_WIDTH / 2} y={height / 2}>Image asset is missing</text>
        )}
        {showGrid && <WorldGrid kind={world.kind} width={STAGE_WIDTH} height={height} zoom={camera.zoom} />}
        {overlay.regions.map((region, index) => (
          <RegionMark
            key={`${region.category}-${index}`}
            region={region}
            width={STAGE_WIDTH}
            height={height}
            zoom={camera.zoom}
          />
        ))}
        {overlay.pins.map((pin, index) => (
          <PinMark
            key={`${index}-${pin.target.kind}`}
            pin={pin}
            width={STAGE_WIDTH}
            height={height}
            zoom={camera.zoom}
          />
        ))}
      </svg>
      <div className="wv-legend">
        {overlay.pins.length} pin{overlay.pins.length === 1 ? '' : 's'} · {overlay.regions.length} region{overlay.regions.length === 1 ? '' : 's'}
      </div>
    </div>
  );
}

function project(point: WorldPoint, width: number, height: number) {
  if (point.kind === 'plane') return { x: point.x * width, y: point.y * height };
  const [x, y] = equalEarthNormalized(point.lon, point.lat);
  return { x: x * width, y: y * height };
}

function fallbackStageToClientPoint(
  stagePoint: { x: number; y: number },
  rectangle: DOMRect,
  camera: WorldCamera,
  height: number,
) {
  return {
    x: rectangle.left + (stagePoint.x - camera.x) / (STAGE_WIDTH / camera.zoom) * rectangle.width,
    y: rectangle.top + (stagePoint.y - camera.y) / (height / camera.zoom) * rectangle.height,
  };
}

function isStagePointInsideRegion(
  region: OverlayRegion,
  point: { x: number; y: number },
  width: number,
  height: number,
) {
  const shape = region.region;
  if (shape.shape !== 'bounding_box') return false;
  const left = shape.x * width;
  const top = shape.y * height;
  return point.x >= left
    && point.x <= left + shape.width * width
    && point.y >= top
    && point.y <= top + shape.height * height;
}

function PinMark({ pin, width, height, zoom }: {
  pin: Pin; width: number; height: number; zoom: number;
}) {
  const position = project(pin.point, width, height);
  const label = pin.target.kind === 'note' ? pin.target.title : `#${pin.target.name}`;
  return (
    <g className="wv-svg-pin" transform={`translate(${position.x} ${position.y})`}>
      <circle r={7 / zoom} />
      <text x={9 / zoom} y={-8 / zoom} fontSize={12 / zoom}>{label || '(untitled)'}</text>
    </g>
  );
}

function RegionMark({ region, width, height, zoom }: {
  region: OverlayRegion; width: number; height: number; zoom: number;
}) {
  const shape = region.region;
  if (shape.shape === 'bounding_box') {
    return (
      <rect
        className="wv-svg-region"
        x={shape.x * width}
        y={shape.y * height}
        width={shape.width * width}
        height={shape.height * height}
        strokeWidth={1.5 / zoom}
      />
    );
  }
  const position = project(region.anchor, width, height);
  return (
    <g className="wv-svg-region-marker" transform={`translate(${position.x} ${position.y})`}>
      <rect x={-4 / zoom} y={-4 / zoom} width={8 / zoom} height={8 / zoom} />
      <text x={7 / zoom} y={4 / zoom} fontSize={11 / zoom}>#{region.category}</text>
    </g>
  );
}
