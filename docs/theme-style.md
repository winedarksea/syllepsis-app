# Nordic/Icelandic Style Guide

This theme layers a specific visual identity on top of Material Design 3: a Scandinavian architect's workspace shaped by Icelandic landscape, saga manuscripts, basalt geometry, moss, meltwater, and restrained Norse knotwork. It should feel calm, precise, sparse, and durable. The theme's drama comes from one or two vivid accents against quiet stone surfaces, not from decoration.

## Design Intent

The governing image is **fire, ice, and stone on a drafting table**:
- **Material Design 3 underneath**: keep MD3 component behavior, state layers, accessibility rules, navigation patterns, and density guidance.
- **Architectural precision**: use grids, hairline rules, flat surfaces, stable spacing, and deliberate alignment.
- **Saga gravity**: reserve historic cues for titles, dividers, graph connections, and transitions. Avoid literal rune fonts or fantasy ornament.
- **Icelandic restraint**: most of the UI is ash, basalt, bone, cool gray, and empty space. Glacial blue, moss, and ember appear only when they mean something.

The theme should never become a themed skin that competes with writing. Notes remain the main artifact.

## Color System

Use Material Design 3 tonal roles, but derive them from a fixed theme palette by default. Dynamic color can exist as an optional user toggle; this theme should not default to wallpaper-matched colors because the identity depends on carefully controlled contrasts.

### Light Mode: Pale Daylight

Light mode should read like ash paper, bleached wood, and stone under northern daylight.

| Role | Name | Hex | Usage |
|---|---:|---:|---|
| Background | Ash White | `#E8E5DE` | Main writing canvas |
| Surface | Bone | `#F3F0E8` | App bars, sheets, inactive panels |
| Surface Variant | Glacial Dust | `#DDE4E6` | Group regions, subtle selected areas |
| Outline | Stone Hairline | `#9A9992` | 1px rules, dividers, inactive graph lines |
| On Surface | Basalt Ink | `#1F2528` | Primary text |
| On Surface Variant | Weathered Slate | `#555E61` | Metadata, labels, secondary text |
| Primary | Geothermal Blue | `#2F7FA3` | Primary actions, active links, selected nodes |
| Secondary | Moss | `#6F8767` | Categories, spatial regions, secondary active state |
| Tertiary | Volcanic Ember | `#A45A36` | Warnings, destructive previews, rare emphasis |
| Error | Iron Red | `#B0433E` | Error role only |

### Dark Mode: Volcanic

Dark mode should not be a dimmed version of light mode. It should feel like basalt ground with meltwater and lichen glowing against it.

| Role | Name | Hex | Usage |
|---|---:|---:|---|
| Background | Obsidian | `#111416` | Main canvas |
| Surface | Basalt | `#20262A` | App bars, panels, notes |
| Surface Variant | Lava Stone | `#2C3438` | Group regions, elevated surfaces |
| Outline | Ash Hairline | `#6E7779` | 1px rules, graph scaffolding |
| On Surface | Ash Text | `#ECE8DE` | Primary text |
| On Surface Variant | Cold Mist | `#B8C0BE` | Metadata, labels, secondary text |
| Primary | Glacial Blue | `#6FA8C7` | Primary actions, active links, selected nodes |
| Secondary | Lichen Moss | `#5E7E58` | Categories, graph weave, map zones |
| Tertiary | Ember | `#C8754D` | Warnings, transient emphasis |
| Error | Heated Iron | `#E0736A` | Error role only |

### Color Rules

- Use tonal surface changes instead of drop shadows. Syllepsis should feel flat, like layered paper and stone.
- Keep the accent budget low: one primary accent per surface, plus secondary only when it carries meaning.
- Do not make large gradients, glow fields, aurora backgrounds, or decorative color blobs.
- Use ember sparingly. It should signal risk, warning, unresolved conflict, or heat beneath the surface.
- Graph and map views may use blue and moss together; book view should usually use one accent at a time.

## Typography

Use three typographic voices, each with a strict role.

| Role | Font Direction | Usage |
|---|---|---|
| Structural Sans | Humanist or geometric sans such as Inter, Jost, or Space Grotesk | Body UI, controls, navigation, dense panels |
| Saga Serif | Restrained flared serif such as Lora, Cormorant Garamond, or Cinzel | Book titles, chapter headings, large empty-state titles |
| Survey Mono | Clean monospace such as IBM Plex Mono or JetBrains Mono | IDs, timestamps, coordinates, sync metadata, diagnostics |

Typography rules:
- Body note text may use the structural sans by default for editing clarity.
- Polished reading/book mode may allow the saga serif for headings and a readable serif or sans for body text, depending on book settings.
- The saga serif is an accent, not the default UI font.
- Metadata, tags, dates, and diagnostic values should be smaller, quieter, and often monospace.
- Avoid faux-runic lettering. Historical reference should come from shape, rhythm, and restraint.

## Shape, Elevation, and Spacing

The base geometry is architectural and basaltic: crisp, grounded, and slightly cut.

- Prefer 0dp-4dp corner radii for cards, note blocks, panels, and input surfaces.
- Use chamfered or clipped corners only for selected hero components such as graph nodes, category chips, or book covers.
- Use MD3 elevation behavior sparingly. Prefer flat tonal separation and hairline outlines.
- Use generous page whitespace in reading and writing surfaces; use tighter density only in diagnostics, search results, and management views.
- Use 1px rules as the default separator. Rules should be basalt/ash hairlines, not high-contrast black or white.
- Avoid nested cards. A note, result, or node can be a card; a page section should not look like a card containing more cards.

## Component Treatment

### App Shell

- Use MD3 top app bars, navigation rails, navigation drawers, FABs, menus, and sheets as the behavioral baseline.
- Top bars should be quiet surfaces with a single hairline bottom rule.
- Navigation rail selected states should use glacial blue or moss tonal fills, not large pills with heavy color.
- Sync status can use simple icon states: synced, syncing, offline, conflict. Green should be moss, not generic bright success green.

### Buttons and Inputs

- Primary actions use glacial/geothermal blue.
- Secondary actions use outlined or tonal MD3 buttons with surface variant fills.
- Destructive or risky actions use ember or error only at confirmation points.
- Inputs should feel like drafting fields: flat, outlined, clear focus ring, no heavy shadows.
- Floating action buttons may keep MD3 shape but should use restrained color and no oversized shadow.

### Icons

Start with Material Symbols or a standard icon set, then tune only where the theme benefits:
- Prefer straight strokes, 45-degree diagonals, and squared terminals for custom icons.
- Do not make icons hard to recognize for the sake of runic style.
- Use runic influence as a geometry constraint, not as literal Futhark characters.

## View-Specific Direction

### Book View: Basalt Column / Saga Page

Book view is the most still and sparse surface. It should signal finality through space, alignment, and calm hierarchy.

- Render sorted notes as a continuous document, not as a feed of heavy cards.
- Use note boundaries only where useful: a thin basalt rule, a small dot-dash divider, or a very low-contrast knotwork divider.
- Avoid shadows and chrome around each note.
- Titles may use the saga serif; metadata should be quiet and often hidden until hover/focus.
- Insert controls should appear as precise drafting marks: a small plus at the join between notes, aligned to the vertical writing rhythm.
- Category headings should feel like chapter markers, with strong spacing before and modest spacing after.

### Graph View: Runic Knot

Graph view earns the theme's identity. It should make connected thoughts feel like modern interlace rather than a messy web.

- Prefer orthogonal or 45-degree edge routing with rounded elbows.
- At line crossings, create a weave effect by masking the lower line with a small background-colored gap around the upper line.
- Use glacial blue for active paths and moss for secondary/category paths.
- Inactive edges should fall back to outline or surface-variant tones.
- Nodes should be simple: pale stone discs, clipped rectangles, or hexagons. The edge weave is the ornament; nodes stay quiet.
- Selected nodes can gain a stronger outline or tonal fill, not a large shadow.
- Dense graphs should degrade gracefully: simplify edge routing, reduce weave decoration, and preserve legibility.
- Semantic clusters use low-opacity survey regions with restrained outlines and short labels; they should read as mapped terrain, not nested cards.
- Prior relationships remain the strongest woven accent lines. Semantic-similarity edges use thinner secondary/moss lines whose opacity reflects strength.
- Density outliers use a dashed outer ring; notes without semantic signal use a dotted ring. These cues must remain legible without color.
- Analysis controls should resemble compact survey instruments: segmented modes, a measured threshold scale, and advanced parameters kept behind disclosure.

### Spatial / Overlay and Map Views: Tundra Grid

Spatial views should feel like a survey map or terrain plan.

- Use a subtle isometric grid, measured square grid, or low-contrast contour lines as the canvas scaffold.
- For image-backed worlds, do not let the theme overpower the imported image or drawing.
- Regions/zones should use surface variant colors, especially ash gray, pale moss, or dark lava stone depending on mode.
- Pins should be small, crisp, and high-contrast enough to find quickly.
- Paths, routes, or relational lines can use glacial blue as a meltwater line.
- Large zones should have soft but not pillowy corners; keep them map-like and restrained.

### Search and Diagnostics

Search, repair, stats, and LLM management views are operational tools. They should be denser and more utilitarian than book view.

- Use the structural sans throughout.
- Use table-like alignment, compact metadata rows, and clear status indicators.
- Keep color semantic: blue for active/selected, moss for healthy/synced/connected, ember for warning, red for error.
- Do not add saga or knotwork ornament to dense workflow surfaces unless it improves wayfinding.

### Empty and Loading States

- Empty states should be quiet and useful: a short title, one action, no illustration unless it helps.
- Loading or sync animations may use a single continuous line drawing itself into a knot and resolving.
- Compiling graph to book can be visualized as a knot untangling into a vertical plumb line, with nodes settling into a stacked manuscript order.

## Motion

Motion should feel weighty and settled, like stone being placed.

- Use MD3 motion duration and easing as the base.
- Prefer smooth settling over bounce.
- Transitions between graph/tree/book can show structural transformation: routes straighten, branches align, notes stack.
- Avoid constant ambient animation. This is a writing app, not a decorative scene.
- Respect reduced-motion settings by replacing spatial transformations with fades and simple state changes.

## Texture and Ornament

Use ornament as a micro-detail.

Appropriate:
- 1px knotwork divider between major book sections.
- Edge weave behavior in graph view.
- Subtle contour/grid lines in spatial views.
- Chamfered selected states or basalt-column hexagons.

Avoid:
- Literal Viking artwork, axes, shields, longships, or rune walls.
- Heavy paper textures, stone textures, or photographic backgrounds in the main UI.
- Decorative gradients, aurora effects, or large ornamental panels.
- Ornament in forms, settings, diagnostics, or any high-frequency workflow.

## Accessibility and Practical Constraints

- Meet WCAG contrast for text and controls in both modes.
- Ensure accent colors remain distinguishable for color-blind users by pairing color with icon, line style, shape, or label.
- Hit targets follow MD3 minimums, especially on tablets and phones.
- Graph weave effects must not be the only cue for relationship direction or status.
- The theme must perform well without heavy images, filters, or expensive canvas effects.
- Let users override typography and contrast where needed.

## Implementation Notes

Represent the theme as tokens, not scattered CSS values:
- MD3 color roles for light and dark palettes.
- Shape scale with crisp defaults and a small number of chamfered variants.
- Typography roles: structural sans, saga serif, survey mono.
- Stroke tokens: hairline, active graph line, emphasized graph line, divider knot line.
- View tokens for graph routing, weave gap size, grid opacity, contour opacity, and node shape.

Suggested theme flags:
```yaml
theme_id: nordic_icelandic
dynamic_color_default: false
elevation_style: flat_tonal
corner_style: crisp
graph_edge_style: orthogonal_weave
spatial_canvas_style: survey_grid
ornament_level: restrained
```

## Design Tests

Before accepting UI work for this theme, check:
- Does the screen still look like Material Design 3 in behavior and hierarchy?
- Is the main surface quiet enough for long writing sessions?
- Are accents meaningful rather than decorative?
- Does light mode feel like ash paper and daylight, while dark mode feels volcanic rather than merely dimmed?
- Are saga/knotwork references subtle and functional?
- Does the screen remain usable with dynamic color disabled, reduced motion enabled, and high contrast requested?


## Future Theme (Not Used as Default, Provided as Alternative to use in Future Theme)
For initial build, we can assume all themes follow some degree of shared use of Material Design 3 patterns.

### The "Navigator’s Archive" Theme (Abstract Medieval / Star Chart)
This theme leans into the romance of discovery, blending the warmth of old manuscripts with the vast, interconnected feel of the cosmos.

Book View (Linear - "The Logbook"): Evokes a scholar's journal. Use warm, off-white, and subtle parchment tones for Surface colors. Typography is key here: use a modern, highly legible Serif font for headlines (like Playfair Display or Merriweather) and a clean Sans-serif for body text.

Graph View (Branching - "The Constellation"): To create contrast, this view shifts entirely to a dark, deep-space palette (indigos, slate, dark navy). The nodes are simple MD3 floating cards, but the connection lines are thin, glowing, gold or brass-toned vectors. Groupings of notes feel like constellations.

Map View (Spatial - "The Cartographer"): Uses muted earth tones, soft oceanic blues, and subtle grid lines or topographic curves in the background.

MD3 Implementation:

    Color Palette: Seed colors of Brass/Gold (Primary), Ink/Indigo (Secondary), and Parchment (Surface).

    Shapes: Sharp, crisp corners (0-4dp radius) for books to mimic cut paper; completely rounded, pill-shaped nodes for graphs to mimic stars/points of light.
