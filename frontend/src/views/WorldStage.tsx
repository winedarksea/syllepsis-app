import { useState } from 'react';
import type { MouseEvent as ReactMouseEvent } from 'react';
import type { Overlay, OverlayRegion, Pin, WorldPoint } from '../types';
import lowDetailEarthUrl from '../assets/earth/countries-equal-earth-low.svg';
import highDetailEarthUrl from '../assets/earth/countries-equal-earth-high.svg';
import { useWorldCamera } from './useWorldCamera';
import { WorldGrid } from './WorldGrid';
import { equalEarthInverseNormalized, equalEarthNormalized } from './worldProjection';

const STAGE_WIDTH = 1200;
const EARTH_HEIGHT = 620;

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
  const [cursorLabel, setCursorLabel] = useState<string | null>(null);
  const viewBox = `${camera.x} ${camera.y} ${STAGE_WIDTH / camera.zoom} ${height / camera.zoom}`;
  const highDetailVisible = world.kind === 'geo' && camera.zoom >= 2.2;

  const updateCursor = (event: ReactMouseEvent<SVGSVGElement>) => {
    const rectangle = event.currentTarget.getBoundingClientRect();
    const stageX = camera.x + (event.clientX - rectangle.left) / rectangle.width * STAGE_WIDTH / camera.zoom;
    const stageY = camera.y + (event.clientY - rectangle.top) / rectangle.height * height / camera.zoom;
    const normalizedX = stageX / STAGE_WIDTH;
    const normalizedY = stageY / height;
    if (world.kind === 'image') {
      setCursorLabel(`x ${normalizedX.toFixed(3)}, y ${normalizedY.toFixed(3)}`);
    } else {
      const coordinate = equalEarthInverseNormalized(normalizedX, normalizedY);
      setCursorLabel(coordinate ? `${coordinate[1].toFixed(3)}°, ${coordinate[0].toFixed(3)}°` : null);
    }
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
        onPointerDown={cameraController.onPointerDown}
        onPointerMove={cameraController.onPointerMove}
        onPointerUp={cameraController.onPointerUp}
        onPointerCancel={cameraController.onPointerUp}
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
            onOpen={() => onOpenCategory(region.category)}
          />
        ))}
        {overlay.pins.map((pin, index) => (
          <PinMark
            key={`${index}-${pin.target.kind}`}
            pin={pin}
            width={STAGE_WIDTH}
            height={height}
            zoom={camera.zoom}
            onOpen={() => pin.target.kind === 'note'
              ? onOpenNote(pin.target.id)
              : onOpenCategory(pin.target.name)}
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

function PinMark({ pin, width, height, zoom, onOpen }: {
  pin: Pin; width: number; height: number; zoom: number; onOpen: () => void;
}) {
  const position = project(pin.point, width, height);
  const label = pin.target.kind === 'note' ? pin.target.title : `#${pin.target.name}`;
  return (
    <g className="wv-svg-pin" transform={`translate(${position.x} ${position.y})`} onClick={onOpen}>
      <circle r={7 / zoom} />
      <text x={9 / zoom} y={-8 / zoom} fontSize={12 / zoom}>{label || '(untitled)'}</text>
    </g>
  );
}

function RegionMark({ region, width, height, zoom, onOpen }: {
  region: OverlayRegion; width: number; height: number; zoom: number; onOpen: () => void;
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
        onClick={onOpen}
      />
    );
  }
  const position = project(region.anchor, width, height);
  return (
    <g className="wv-svg-region-marker" transform={`translate(${position.x} ${position.y})`} onClick={onOpen}>
      <rect x={-4 / zoom} y={-4 / zoom} width={8 / zoom} height={8 / zoom} />
      <text x={7 / zoom} y={4 / zoom} fontSize={11 / zoom}>#{region.category}</text>
    </g>
  );
}
