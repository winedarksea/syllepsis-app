# UI & Views

## Core Views

### Book View
Sorted notes rendered as a single continuous document. The primary reading and writing surface for polished content. See [core-concepts.md](core-concepts.md) for the sorting model.

### Hybrid Book-Tree-Graph View (large screens)
Sorted notes in the main column; unsorted notes linked by shared categories branching off to the side. The side panel scrolls independently from the book column. Requires careful design given potentially many linked unsorted notes.

### Graph View
Notes as nodes with four organization lenses:

- **Categories** preserves the declared first-category grouping.
- **Pillars** uses a broad semantic projection and fixed-count clustering to reveal the book's main themes.
- **Communities** emphasizes local semantic neighborhoods and community detection to reveal tightly connected interests.
- **Density** identifies well-developed semantic regions while marking isolated notes as outliers.

Semantic-similarity edges are controlled by an interactive threshold; explicit prior relationships remain visible with a distinct stronger line. Cluster regions are exploratory and never rewrite categories or note relationships. The canvas supports pan, zoom, title visibility controls, and advanced tuning for the active analysis mode.

### Category View
Starts at the category level rather than the note level. Users browse categories first and drill into notes from there.

### Unsorted / Uncategorized Queue
Dedicated view for quick notes awaiting organization. Focused on categorizing, deduplicating, and refining raw captures into the overall structure.

### Search View
See [search.md](search.md) for full search details. Once a search is entered, the results are shown as a web of related content centered on the query. Options to:
- Start a chat with an LLM using selected context
- Click into notes to read or edit

### Related Carousel
A note is displayed with similar notes surrounding it — similarity ranked by vector distance, with category membership used as an upweight.

### Constellation / Star Chart View
A visual "star chart" style display. Zooming into a cluster reveals a "solar system" style sub-view of a category and its notes.

### Spatial / Overlay View
Renders an [image-backed world](spatial-worlds.md#worlds) — a floorplan, a drawing, or any imported image — with an overlay of pins and clickable regions for notes and categories placed in it. Clicking a region (e.g. the `#kitchen` area of a floorplan) opens the linked notes or runs the category's [filtered sorted view](core-concepts.md#filtered-sorted-view). This is the primary lens for **mind palaces** and floorplan-tagged house books. Like the timeline, it is a view over location metadata, not a data type. Available in the first pass for image-backed worlds.

### Map View (future)
A future view that loads map tiles for **geo worlds** (Earth and user-defined planets) and shows every geo-tagged note as a pin; clicking a pin opens the note. Deferred to a later pass — it needs tile infrastructure, whereas the Spatial/Overlay view above does not. See [spatial-worlds.md](spatial-worlds.md#map-view-future).

### Timeline View (future)
A future view that lays notes out along their date metadata. It is a UI view, not a data type — it renders the dates already stored on notes (see [object-types.md](object-types.md#date-metadata)).

### Stats Dashboard
Analytics view showing:
- Vector alignment between views
- Note update times
- How often a note has been opened by the user
- How often a note has been retrieved by LLMs
- Other analytics used to rank note usefulness

### Diagnostics & Repair View
A dedicated tab grouping maintenance tools:
- **Broken links**: find and fix links that no longer resolve
- **Orphaned notes**: notes with no category or prior connection
- **Blind spot detection**: notes with the lowest vector similarity to their neighbors (suggests narrative gaps; see [llm-ai-features.md](llm-ai-features.md))

### LLM Management View
- Manage prompts and prompt templates
- Configure API tokens and provider connections
- Route specific task types (summarization, fact-check, etc.) to specific providers or local models

### Privacy & Policy View
A centralized UI for tagging categories as private or locked and configuring access control. See [privacy-security.md](privacy-security.md).

### Read-Only Server View
A published website view (separate port from the edit view). Supports search and reading but not editing. Likely a PWA as well. The edit port stays private; the read-only port can be exposed to the internet.

## Adding Notes

Two entry points:
- **Insert between notes**: a "+" icon appears on hover near the link between two sorted notes, allowing a new note to be inserted in that position.
- **New unsorted note**: a "New Note" button creates an unsorted note. If another note is currently selected, the new note inherits its categories by default.

## Editing & Links

- Links can point to sections inside a text object using markdown headers (e.g. `note-id#section-heading`). The general goal is to break large text into small individual objects, but section links exist for cases where keeping content together makes sense — e.g. importing an essay from a blog and preserving it as written.
- The long-term goal is WYSIWYG markdown. The initial POC does not require it.
- When a user drags or reorganizes notes inside a category on Android (touch), the UI supports drag-and-drop reordering.

## Device & Platform UX

- Editing views adapt to device size (laptop, tablet, phone).
- A common workflow: type on a laptop, then review and drag-reorganize on a tablet.
- New users with no books are prompted to download example books.
- Native spell check and auto-save on all platforms.

## Theming

Multiple themes supported, including custom user themes. Starting themes: light mode and dark mode. The first named visual direction is the [Nordic/Icelandic Material Design 3 theme](nordic-icelandic-style.md), which defines color, typography, shape, graph, book, and spatial-view treatment.

Icons/covers on each book for visual distinction.
Sync status (if linked to cloud) should be clearly displayed, likely a simple icon (green for all synced to cloud).
