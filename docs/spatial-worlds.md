# Spatial Worlds & Overlays

Notes can be placed in space, not just in the narrative tree. This powers three related use cases with a single primitive:

- **Geo-tagging** — pin a note to a real Earth lat/long (e.g. a reference photo taken on site).
- **Floorplan tagging** — overlay `#kitchen` and related notes onto a floorplan image of a house book.
- **Mind palaces** — tag notes into locations on an image (method of loci) to aid memory and generative learning.

All three are the same model: a **world** (a coordinate space with an optional backdrop) plus **locations** (named points or regions in that world) that notes and categories link to. A real map, a floorplan, and a memory palace differ only in what kind of world backs them.

## Worlds

A **world** is a coordinate space notes can be placed in. There are two kinds:

| Kind | Coordinate space | Backdrop | Examples |
|---|---|---|---|
| **Geo** | lat / long on a sphere | bundled projected SVG | `earth` (default) |
| **Image-backed (plane)** | 2D plane, normalized `(x, y)` in `0..1` | a [drawing (SVG)](object-types.md#drawings) or [raster image](object-types.md#pictures) | a floorplan of the first floor; a hand-drawn memory palace |

- `earth` is the default geo world. It uses a bundled, offline **Equal Earth** SVG basemap generated
  from Natural Earth Admin-0 country geometry. A compact global layer is shown initially and a
  bundled 1:10m layer is used at continent-level zoom; there are no tiles or runtime downloads.
- An **image-backed world** is just an image plus a coordinate frame. A floorplan of one floor of a house is a world; a multi-floor house is several worlds (one per floor), optionally grouped.
- Image-backed worlds store **normalized** coordinates (`0..1` of the image's intrinsic width/height) rather than pixels, so locations survive the backdrop being re-exported at a different resolution.

### Worlds registry

Each world has a small metadata entry (a markdown/frontmatter file in the book, alongside `_categories/`):

- `id`, `display_name`
- `kind`: `geo | image`
- For `image`: `backdrop` — a UUID reference to the backing drawing/image object — plus its intrinsic dimensions
- For `geo`: an optional `tile_source` URL

The existing **text → coordinate lookup table** ([object-types.md](object-types.md#location-metadata)) resolves named places (`"the kitchen"`, `"job site"`) to coordinates and carries a `world` column, so a plain-text location string keeps working and simply resolves within its world.

The Worlds view creates image worlds from an existing Picture/Drawing object or a newly imported
backdrop. Rust derives a collision-safe id, verifies the asset UUID and dimensions, and only then
writes the registry entry. Earth remains immutable. Deletion is blocked while notes, categories,
regions, or lookup rows still reference the world; the backing image object is retained.

## Coordinates in markdown (`loc:` syntax)

Location is an **inline** element, not a block object type — it can sit mid-sentence or inside a table cell. It uses `key:value` syntax sugar consistent with todo dates (`due:`), and typing `loc:` opens a location picker (map for geo worlds, the backdrop image for image worlds), mirroring how `due:` opens a calendar.

```
loc:47.6062,-122.3321          # Earth lat,long (world defaults to earth)
loc:earth/47.6062,-122.3321    # explicit geo world
loc:firstfloor/0.42,0.31       # image-backed world, normalized x,y
loc:@kitchen                   # named place, resolved via the lookup table
```

- Geo coordinates are `lat,long`. Image-world coordinates are normalized `x,y`.
- World is optional and defaults to `earth`; omit it for plain Earth coordinates.

### Note-level location metadata

A whole note can be pinned by adding an optional `location` field to its frontmatter (same value forms as `loc:` above). This is how a note appears as a single pin on a map or overlay without an inline token in the body. A note may carry both a note-level location and inline `loc:` tokens (e.g. a trip log that references several sites).

### Categories with a location

A category may carry an optional location — a point or a **region** — in a world. This is what makes `#kitchen` a clickable area on a floorplan: clicking the kitchen region runs the existing [filtered sorted view](core-concepts.md#filtered-sorted-view) for `#kitchen`. Categories already support icons; a location is the spatial counterpart.

## Overlays

Any image-backed world renders with an **overlay layer**: pins (points) and regions that link to notes and categories.

- **Regions** define clickable areas, not just dots:
  - **SVG / drawing backdrops** (preferred) — a named SVG element (`id="kitchen"`) is itself the region. Imported SVG floorplans get clickable rooms essentially for free.
  - **Raster backdrops** — a region is an app-stored polygon/bbox in normalized coordinates.
- **Zoom scaling.** Pins and regions are anchored in normalized coordinates so they stay locked to the right spot at any zoom. For SVG this is automatic (vector). **For raster images the overlay must apply an explicit zoom/pan transform** so pins track the backdrop as the user zooms in and out — by default pins hold a constant on-screen size while staying anchored, with a toggle to scale with the content.
- **Coordinate grid.** Earth can show a numbered adaptive latitude/longitude graticule. Image
  worlds use normalized X/Y labels. Cursor coordinates use the same convention as `loc:`.

## Earth map

Earth is rendered in the same SVG pan/zoom canvas as image worlds. Equal Earth projection math is
implemented in Rust and mirrored by tested frontend helpers. The committed map assets contain
country/coast geometry only: no roads, admin-1 boundaries, rivers, cities, labels, or tile source.

## Mind palaces

A **mind palace** is not a new data type — it is a book (or [knowledge pack](core-concepts.md#knowledge-packs)) viewed through the lens of an image-backed world with an overlay, used as a method-of-loci memory aid. SVG is the preferred backdrop (clean zoom, named regions). This is a fourth organizational lens alongside graph / tree / book, and it reinforces the vision's [generative-learning](vision.md) goal — loci is a memory technique, not a bolt-on.

## Relationship to drawings

Imported **SVGs are treated as [drawings](object-types.md#drawings)**, and the future in-app drawing tool will also emit SVG. Both therefore share this same overlay-and-anchor tooling — a hand-drawn palace and an imported floorplan are handled identically. See [object-types.md](object-types.md#drawings) for the drawing object type and the [SVG/CRDT question](sync-backup.md#drawings-and-svg) for how drawing geometry is synced.

## Phasing

1. **Implemented** — first-class Picture/Drawing ingestion; image-world creation; overlay pins and
   regions; `loc:` syntax; adaptive grids; shared SVG pan/zoom; and the offline Equal Earth map.
2. **Future** — optional projections, richer geographic layers, the in-app drawing tool, and
   multi-floor world grouping. Street-level tile navigation remains outside the intended precision.

The authoring of detailed floorplans is **out of scope** for Syllepsis — that lives in the separate `catlin-house` IFC/BIM tooling, which already renders floorplan images. Syllepsis *imports* a floorplan as a world backdrop rather than reimplementing a CAD editor.
