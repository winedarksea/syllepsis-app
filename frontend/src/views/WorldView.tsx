// Spatial overlay view (Phase 5). Renders a world's pins and regions over its backdrop. Image
// worlds (floorplans, mind palaces) are the first-pass target; geo worlds fall back to a simple
// equirectangular projection until the map-tile view lands (a later phase).
//
// The backdrop image/SVG is fetched from the core as a self-contained data URL and drawn behind
// the overlay; pins and regions are anchored by their normalized 0..1 coordinates so they sit
// correctly over it. When a world has no backdrop on disk yet, a labeled placeholder is shown.

import { useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import { Icon } from '../components/Icon';
import type { Overlay, Pin, OverlayRegion, World, WorldPoint } from '../types';
import './WorldView.css';

/** Project any world coordinate into a normalized (x, y) in 0..1 for absolute positioning. */
function project(point: WorldPoint): { x: number; y: number } {
  if (point.kind === 'plane') return { x: point.x, y: point.y };
  // Equirectangular projection for geo points (north-up).
  return { x: (point.lon + 180) / 360, y: (90 - point.lat) / 180 };
}

function pct(n: number): string {
  return `${(n * 100).toFixed(3)}%`;
}

export function WorldView() {
  const { activeWorld, setActiveWorld, openEditor, setActiveCategory, setView } = useStore();
  const [worlds, setWorlds] = useState<World[]>([]);
  const [overlay, setOverlay] = useState<Overlay | null>(null);
  const [backdropByWorld, setBackdropByWorld] = useState<{ worldId: string; data: string | null } | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Load the world list once; default the active world to the first image world (or earth).
  useEffect(() => {
    api.listWorlds()
      .then((ws) => {
        setWorlds(ws);
        if (!activeWorld && ws.length > 0) {
          const preferred = ws.find((w) => w.kind === 'image') ?? ws[0];
          setActiveWorld(preferred.id);
        }
      })
      .catch((e) => setError(String(e)));
  }, [activeWorld, setActiveWorld]);

  // Load the overlay and backdrop whenever the active world changes.
  useEffect(() => {
    if (!activeWorld) return;
    api.worldOverlay(activeWorld)
      .then((o) => { setOverlay(o); setError(null); })
      .catch((e) => setError(String(e)));
    api.worldBackdrop(activeWorld)
      .then((data) => setBackdropByWorld({ worldId: activeWorld, data }))
      .catch(() => setBackdropByWorld({ worldId: activeWorld, data: null }));
  }, [activeWorld]);

  const openCategory = useCallback((name: string) => {
    setActiveCategory(name);
    setView('category');
  }, [setActiveCategory, setView]);

  if (error) return <div className="wv-state wv-error">{error}</div>;
  if (worlds.length === 0) return <div className="wv-state">No worlds defined yet.</div>;

  const world = overlay?.world;
  const isImage = world?.kind === 'image';
  // Keep the plane's aspect ratio to the backdrop's intrinsic size when known.
  const aspect = world?.intrinsic_dimensions
    ? world.intrinsic_dimensions[0] / world.intrinsic_dimensions[1]
    : 16 / 9;
  const backdrop = backdropByWorld?.worldId === activeWorld ? backdropByWorld.data : null;

  return (
    <div className="wv-root">
      <div className="wv-header">
        <h2 className="wv-title">Worlds</h2>
        <div className="wv-world-tabs">
          {worlds.map((w) => (
            <button
              key={w.id}
              className={`wv-world-tab ${activeWorld === w.id ? 'active' : ''}`}
              onClick={() => setActiveWorld(w.id)}
            >
              <Icon className="wv-world-kind" name={w.kind === 'image' ? 'map' : 'public'} size={16} />
              {w.display_name}
            </button>
          ))}
        </div>
      </div>

      {overlay && (
        <div className="wv-stage-wrap">
          <div
            className="wv-stage"
            style={{ aspectRatio: String(aspect) }}
            data-kind={world?.kind}
          >
            {backdrop ? (
              <img className="wv-backdrop-img" src={backdrop} alt={`${world?.display_name ?? ''} backdrop`} />
            ) : (
              <div className="wv-backdrop-note">
                {isImage
                  ? `backdrop: ${world?.backdrop ?? '(none set)'} — no image asset on disk yet`
                  : 'geo world — equirectangular projection (map tiles are a later phase)'}
              </div>
            )}

            {overlay.regions.map((r, i) => (
              <RegionMark key={`r-${i}`} region={r} onOpen={() => openCategory(r.category)} />
            ))}

            {overlay.pins.map((p, i) => (
              <PinMark
                key={`p-${i}`}
                pin={p}
                onOpen={() =>
                  p.target.kind === 'note'
                    ? openEditor(p.target.id)
                    : openCategory(p.target.name)
                }
              />
            ))}
          </div>

          <div className="wv-legend">
            {overlay.pins.length} pin{overlay.pins.length !== 1 ? 's' : ''} ·{' '}
            {overlay.regions.length} region{overlay.regions.length !== 1 ? 's' : ''}
            {overlay.pins.length === 0 && overlay.regions.length === 0 && (
              <span className="wv-empty"> — tag notes with a <code>loc:</code> token to place them here.</span>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function PinMark({ pin, onOpen }: { pin: Pin; onOpen: () => void }) {
  const { x, y } = project(pin.point);
  const label = pin.target.kind === 'note' ? pin.target.title : `#${pin.target.name}`;
  return (
    <button
      className={`wv-pin wv-pin-${pin.target.kind}`}
      style={{ left: pct(x), top: pct(y) }}
      onClick={onOpen}
      title={label}
    >
      <span className="wv-pin-dot" />
      <span className="wv-pin-label">{label || '(untitled)'}</span>
    </button>
  );
}

function RegionMark({ region, onOpen }: { region: OverlayRegion; onOpen: () => void }) {
  const r = region.region;
  if (r.shape === 'bounding_box') {
    return (
      <button
        className="wv-region"
        style={{ left: pct(r.x), top: pct(r.y), width: pct(r.width), height: pct(r.height) }}
        onClick={onOpen}
        title={`#${region.category}`}
      >
        <span className="wv-region-label">#{region.category}</span>
      </button>
    );
  }
  // SVG-element and polygon regions have no drawable box here (the SVG/raster geometry lives in the
  // backdrop); anchor a clickable marker at the category's anchor point instead.
  const { x, y } = project(region.anchor);
  return (
    <button
      className="wv-region wv-region-marker"
      style={{ left: pct(x), top: pct(y) }}
      onClick={onOpen}
      title={`#${region.category} (${r.shape === 'svg_element' ? r.element_id : 'polygon'})`}
    >
      <span className="wv-region-label">#{region.category}</span>
    </button>
  );
}
