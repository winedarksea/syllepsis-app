// Bundled curated icon sets for theme signature slots.
// Each glyph is { viewBox, path, cap? } — monochrome stroke icons rendered by the Icon
// component at 1.5px stroke on a 24×24 grid. 'material' is intentionally empty so slots
// fall back to the Material Symbols ligature.
//
// Design rules (docs/theme-style.md): every glyph must read instantly as its function;
// the theme shows up as one restrained cue per icon, never as ornament that costs clarity.

import type { SignatureSlot, ThemeIcon } from '../themes';

export type IconSet = Partial<Record<SignatureSlot, ThemeIcon>>;

// ── Nordic / Icelandic ────────────────────────────────────────────────────────
// Drafting-table geometry: straight strokes, 45° diagonals, squared (butt) terminals.
// No curves anywhere — circles become octagons, clouds become basalt facets.
const N = (path: string): ThemeIcon => ({ viewBox: '0 0 24 24', path, cap: 'butt' });

const NORDIC: IconSet = {
  // Open book: angular raised covers, ruled saga lines on each page.
  book: N(
    'M12 5.5 L4 4 L4 17.5 L12 19.5 L20 17.5 L20 4 Z ' +
    'M12 5.5 L12 19.5 ' +
    'M6.5 8 L9.5 8.6 M6.5 11 L9.5 11.6 ' +
    'M14.5 8.6 L17.5 8 M14.5 11.6 L17.5 11',
  ),
  // Inbox tray: standard silhouette, 45°-cut notch.
  unsorted: N(
    'M4 4.5 L20 4.5 L20 19.5 L4 19.5 Z ' +
    'M4 13 L8 13 L10.5 15.5 L13.5 15.5 L16 13 L20 13',
  ),
  // Magnifier with a basalt-column octagon lens and 45° handle.
  search: N(
    'M7.3 3.5 L12.7 3.5 L16.5 7.3 L16.5 12.7 L12.7 16.5 L7.3 16.5 L3.5 12.7 L3.5 7.3 Z ' +
    'M14.7 14.7 L20.5 20.5',
  ),
  // Hub: diamond nodes joined by vertical + 45° spokes.
  graph: N(
    'M12 9.5 L14.5 12 L12 14.5 L9.5 12 Z ' +
    'M12 2.5 L14 4.5 L12 6.5 L10 4.5 Z ' +
    'M5.5 16.5 L7.5 18.5 L5.5 20.5 L3.5 18.5 Z ' +
    'M18.5 16.5 L20.5 18.5 L18.5 20.5 L16.5 18.5 Z ' +
    'M12 9.5 L12 6.5 ' +
    'M10.75 13.25 L6.5 17.5 ' +
    'M13.25 13.25 L17.5 17.5',
  ),
  // Folded survey map with a small plotting cross on the centre panel.
  worlds: N(
    'M3 5 L9 3.5 L15 5.5 L21 4 L21 19 L15 20.5 L9 18.5 L3 20 Z ' +
    'M9 3.5 L9 18.5 M15 5.5 L15 20.5 ' +
    'M12 10.5 L12 13.5 M10.5 12 L13.5 12',
  ),
  // Isometric crate with a strap seam across the upper faces.
  packs: N(
    'M4 8 L12 4.5 L20 8 L20 16 L12 19.5 L4 16 Z ' +
    'M4 8 L12 11.5 L20 8 M12 11.5 L12 19.5 ' +
    'M8 6.25 L16 9.75',
  ),
  // Add: a plain drafting cross — precision over ornament.
  new: N('M12 5 L12 19 M5 12 L19 12'),
  // Cloud sync: opposed transfer arrows, 45° heads.
  sync: N(
    'M4 8.5 L20 8.5 M16.5 5 L20 8.5 L16.5 12 ' +
    'M20 15.5 L4 15.5 M7.5 12 L4 15.5 L7.5 19',
  ),
  // Git sync: two branches merging at 45° into a down arrow.
  sync_git: N(
    'M7 4.5 L7 9.5 L12 14.5 ' +
    'M17 4.5 L17 9.5 L12 14.5 ' +
    'M12 14.5 L12 20 ' +
    'M9.7 17.7 L12 20 L14.3 17.7',
  ),
  // Offline: faceted basalt cloud, struck through at 45°.
  sync_off: N(
    'M6 17 L4 15 L4 12.5 L6.5 10 L9.5 10 L12.5 7 L14.5 7 L17.5 10 L19 10 L21 12 L21 15 L19 17 Z ' +
    'M4.5 20.5 L20.5 4.5',
  ),
};

// ── Navigator's Archive ───────────────────────────────────────────────────────
// Star-chart geometry: circles, arcs, and small four-pointed stars; round terminals.
const A = (path: string): ThemeIcon => ({ viewBox: '0 0 24 24', path });

// Four-pointed star (point of light) centred on (cx, cy) with radius r.
const star4 = (cx: number, cy: number, r: number): string => {
  const w = r * 0.28;
  return (
    `M${cx} ${cy - r} L${cx + w} ${cy - w} L${cx + r} ${cy} L${cx + w} ${cy + w} ` +
    `L${cx} ${cy + r} L${cx - w} ${cy + w} L${cx - r} ${cy} L${cx - w} ${cy - w} Z`
  );
};

// Circle as two arcs (single-arc circles collapse to nothing in SVG).
const circle = (cx: number, cy: number, r: number): string =>
  `M${cx - r} ${cy} A${r} ${r} 0 1 1 ${cx + r} ${cy} A${r} ${r} 0 1 1 ${cx - r} ${cy}`;

const ARCHIVE: IconSet = {
  // Open logbook: soft page curves, ruled lines left, a plotted star right.
  book: A(
    'M12 6.5 C10.5 4.8 7.5 4 3.5 4.3 L3.5 18 C7.5 17.7 10.5 18.5 12 20 ' +
    'C13.5 18.5 16.5 17.7 20.5 18 L20.5 4.3 C16.5 4 13.5 4.8 12 6.5 Z ' +
    'M12 6.5 L12 20 ' +
    'M5.5 8.5 L9.5 9.2 M5.5 11.5 L9.5 12.2 ' +
    star4(16.2, 10, 1.5),
  ),
  // Inbox: rounded tray with an arced notch; one uncatalogued star waiting inside.
  unsorted: A(
    'M4 5.5 Q4 4.5 5 4.5 L19 4.5 Q20 4.5 20 5.5 L20 18.5 Q20 19.5 19 19.5 L5 19.5 Q4 19.5 4 18.5 Z ' +
    'M4 13 L8.5 13 Q9.5 15.5 12 15.5 Q14.5 15.5 15.5 13 L20 13 ' +
    star4(12, 8.5, 1.7),
  ),
  // Spyglass lens fixed on a star.
  search: A(
    circle(10.5, 10, 6.5) + ' M15.2 14.7 L20.5 20 ' + star4(10.5, 10, 2.1),
  ),
  // Constellation: the Dipper — a four-star bowl with a handle rising to one bright
  // four-point star. Reads as an asterism and as a graph (a cycle plus a path).
  graph: A(
    circle(4.5, 18, 1.6) + ' ' + circle(11, 19, 1.3) + ' ' +
    circle(12.5, 12.5, 1.5) + ' ' + circle(5, 11.5, 1.2) + ' ' +
    star4(19.5, 5.5, 2.4) + ' ' +
    'M6.4 18.3 L9.4 18.8 M11.4 17.4 L12.1 14.3 M10.7 12.3 L6.5 11.7 ' +
    'M4.9 13 L4.6 16.1 M13.8 11.2 L17.5 7.5',
  ),
  // Globe with equator and meridian — the cartographer's baseline.
  worlds: A(
    circle(12, 12, 8.5) + ' M3.5 12 L20.5 12 ' +
    'M12 3.5 A4.3 8.5 0 1 1 12 20.5 A4.3 8.5 0 1 1 12 3.5',
  ),
  // Sea chest: arched lid, seam, and hasp.
  packs: A(
    'M4 12 L4 19 Q4 20 5 20 L19 20 Q20 20 20 19 L20 12 ' +
    'M4 12 L4 10 Q4 6 9 6 L15 6 Q20 6 20 10 L20 12 ' +
    'M4 12 L20 12 ' +
    'M10.8 12 L10.8 14.5 Q10.8 15.5 12 15.5 Q13.2 15.5 13.2 14.5 L13.2 12',
  ),
  // Add: compass-rose cross — a plus with a small rotated-diamond hub.
  new: A(
    'M12 4 L12 20 M4 12 L20 12 ' +
    'M12 9.8 L14.2 12 L12 14.2 L9.8 12 Z',
  ),
  // Cloud sync: circular course-correction arrows.
  sync: A(
    'M20.5 5 L20.5 10 L15.5 10 ' +
    'M3.5 19 L3.5 14 L8.5 14 ' +
    'M4.9 9.5 A7.5 7.5 0 0 1 17.3 6.7 L20.5 9.7 ' +
    'M19.1 14.5 A7.5 7.5 0 0 1 6.7 17.3 L3.5 14.3',
  ),
  // Git sync: two commit stars merging into one.
  sync_git: A(
    circle(7, 5, 1.6) + ' ' + circle(17, 5, 1.6) + ' ' + circle(12, 19, 1.6) + ' ' +
    'M7 6.6 L7 9.5 Q7 12.5 9.6 13.6 L12 14.6 ' +
    'M17 6.6 L17 9.5 Q17 12.5 14.4 13.6 L12 14.6 ' +
    'M12 14.6 L12 17.4',
  ),
  // Offline: cloud bank struck through.
  sync_off: A(
    'M8.5 18.5 A6 6 0 1 1 14.2 10.6 L16.5 10.6 A3.95 3.95 0 0 1 16.5 18.5 Z ' +
    'M4.5 20 L20 4.5',
  ),
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
