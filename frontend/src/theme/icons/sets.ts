// Bundled curated icon sets for theme signature slots.
// Each glyph is { viewBox, path } — monochrome, fill="currentColor", sized by the Icon component.
// 'material' is intentionally empty so slots fall back to the Material Symbols ligature.

import type { SignatureSlot, ThemeIcon } from '../themes';

export type IconSet = Partial<Record<SignatureSlot, ThemeIcon>>;

// ── Nordic / Icelandic — straight-stroke, runic-geometry glyphs ──────────────
// All drawn on a 24×24 grid; angular, no curves.
const NORDIC: IconSet = {
  // Open book: two angular pages meeting at a spine with runic lines
  book: {
    viewBox: '0 0 24 24',
    path: 'M12 4 L4 6 L4 20 L12 18 L12 4 Z M12 4 L20 6 L20 20 L12 18 L12 4 Z M6 9 L10 8.2 M6 12 L10 11.2 M6 15 L10 14.2 M14 8.2 L18 9 M14 11.2 L18 12 M14 14.2 L18 15',
  },
  // Inbox/tray: angular rune-tray shape
  unsorted: {
    viewBox: '0 0 24 24',
    path: 'M3 5 L21 5 L21 14 L17 14 L17 17 L7 17 L7 14 L3 14 Z M3 5 L7 11 M21 5 L17 11',
  },
  // Search: magnifying glass with angular handle and crosshairs
  search: {
    viewBox: '0 0 24 24',
    path: 'M10 3 L10 17 M3 10 L17 10 M14.5 14.5 L20 20 M7 10 A3 3 0 1 1 13 10 A3 3 0 1 1 7 10',
  },
  // Graph/hub: hexagonal node cluster, runic spokes
  graph: {
    viewBox: '0 0 24 24',
    path: 'M12 12 L12 4 M12 12 L18.9 16 M12 12 L5.1 16 M12 12 L4 8 M12 12 L20 8 M12 4 L12 2 M18.9 16 L20 17 M5.1 16 L4 17 M4 8 L3 7 M20 8 L21 7 M12 4 m-1.5 0 l1.5-2 l1.5 2 l-1.5 2 Z',
  },
  // Worlds/map: angular grid with compass rose centre
  worlds: {
    viewBox: '0 0 24 24',
    path: 'M3 3 L21 3 L21 21 L3 21 Z M3 9 L21 9 M3 15 L21 15 M9 3 L9 21 M15 3 L15 21 M12 10 L12 14 M10 12 L14 12',
  },
  // Packs: crate with runic studs
  packs: {
    viewBox: '0 0 24 24',
    path: 'M3 8 L12 4 L21 8 L21 20 L3 20 Z M3 8 L3 20 M21 8 L21 20 M3 8 L12 12 L21 8 M12 12 L12 20 M7 14 L7 14 M17 14 L17 14',
  },
  // New/add: runic plus on angular tile
  new: {
    viewBox: '0 0 24 24',
    path: 'M12 5 L12 19 M5 12 L19 12',
  },
  // Sync: angular arrows forming a square circuit
  sync: {
    viewBox: '0 0 24 24',
    path: 'M4 8 L12 4 L20 8 M4 16 L12 20 L20 16 M4 8 L4 16 M20 8 L20 16 M9 12 L12 9 L15 12 M12 9 L12 15',
  },
};

// ── Navigator's Archive — star-chart / brass compass glyphs ──────────────────
// All drawn on a 24×24 grid; circular arcs, points-of-light, sextant motifs.
const ARCHIVE: IconSet = {
  // Open logbook with compass rose on left page
  book: {
    viewBox: '0 0 24 24',
    path: 'M3 6 Q3 4 5 4 L12 5 L12 19 L5 20 Q3 20 3 18 Z M21 6 Q21 4 19 4 L12 5 L12 19 L19 20 Q21 20 21 18 Z M7 11 A2 2 0 1 1 7 11.01 M7 9 L7 8 M7 13 L7 14 M5 11 L4 11 M9 11 L10 11 M5.6 9.6 L5 9 M8.4 12.4 L9 13 M8.4 9.6 L9 9 M5.6 12.4 L5 13 M15 8 L17 8 M15 11 L17 11 M15 14 L17 14',
  },
  // Inbox tray with star above
  unsorted: {
    viewBox: '0 0 24 24',
    path: 'M3 11 L3 20 L21 20 L21 11 M2 11 L22 11 M9 14 L15 14 M12 4 L13.2 7.6 L17 7.6 L14 9.8 L15.2 13.4 L12 11.2 L8.8 13.4 L10 9.8 L7 7.6 L10.8 7.6 Z',
  },
  // Magnifying glass with star-chart crosshairs
  search: {
    viewBox: '0 0 24 24',
    path: 'M10 3 A7 7 0 1 1 10 17 A7 7 0 1 1 10 3 M10 6 L10 14 M6 10 L14 10 M14.9 14.9 L21 21',
  },
  // Star hub: central 4-pointed star with orbital ring
  graph: {
    viewBox: '0 0 24 24',
    path: 'M12 2 L13.5 10.5 L21 12 L13.5 13.5 L12 22 L10.5 13.5 L2 12 L10.5 10.5 Z M12 12 A5 5 0 1 1 12 11.99',
  },
  // World map: globe with meridians and a compass star
  worlds: {
    viewBox: '0 0 24 24',
    path: 'M12 3 A9 9 0 1 1 12 21 A9 9 0 1 1 12 3 M3 12 L21 12 M12 3 Q15 7 15 12 Q15 17 12 21 Q9 17 9 12 Q9 7 12 3 M12 10 L12.7 11.3 L14.2 11.5 L13.1 12.6 L13.4 14.1 L12 13.4 L10.6 14.1 L10.9 12.6 L9.8 11.5 L11.3 11.3 Z',
  },
  // Chest / pack with brass corner fittings
  packs: {
    viewBox: '0 0 24 24',
    path: 'M3 9 L3 20 L21 20 L21 9 Q21 8 20 8 L4 8 Q3 8 3 9 Z M3 9 L21 9 M11 14 L13 14 L13 12 L11 12 L11 14 M5 9 A2 2 0 0 1 5 9.01 M19 9 A2 2 0 0 1 19 9.01 M5 20 A2 2 0 0 1 5 20.01 M19 20 A2 2 0 0 1 19 20.01 M8 4 L8 8 M16 4 L16 8 M8 4 L16 4',
  },
  // New/add: compass point cross
  new: {
    viewBox: '0 0 24 24',
    path: 'M12 3 L12 21 M3 12 L21 12 M12 3 L13 5 L12 4 L11 5 M12 21 L13 19 L12 20 L11 19 M3 12 L5 11 L4 12 L5 13 M21 12 L19 11 L20 12 L19 13',
  },
  // Sync: celestial orrery — two arcs with arrowheads
  sync: {
    viewBox: '0 0 24 24',
    path: 'M12 12 A7 7 0 0 1 19 12 M19 12 L17 10 M19 12 L21 10 M12 12 A7 7 0 0 0 5 12 M5 12 L7 14 M5 12 L3 14 M12 5 A1.5 1.5 0 1 1 12 5.01 M12 19 A1.5 1.5 0 1 1 12 19.01',
  },
};

// 'material' set is empty — Icon falls back to Material Symbols ligature for every slot.
const MATERIAL: IconSet = {};

const SETS: Record<string, IconSet> = {
  material: MATERIAL,
  nordic: NORDIC,
  archive: ARCHIVE,
};

export function getIconSet(id: string): IconSet {
  return SETS[id] ?? MATERIAL;
}
