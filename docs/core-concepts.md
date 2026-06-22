# Core Concepts

## Unsorted vs. Sorted Notes

Notes begin life as **unsorted** — quick captures with no fixed position in the overall structure. The primary goal is to make it easy to collect thoughts first and organize later.

A **sorted** note has a clear place in the book view: it belongs to a specific location in the narrative sequence. An **unsorted** note does not have a fixed position but can still have categories.

**Branches** are a hybrid: a cluster of notes that are sorted relative to each other but not yet placed in the main narrative. For example, five related items grouped together that haven't been woven into the main text yet.

## The Organization Ladder

Notes move through levels of organization over time — users can stop at any level that suits them:

```
Uncategorized (raw capture)
    ↓
Categorized (graph view — notes linked by shared categories)
    ↓
Tree (notes organized into a hierarchy)
    ↓
Book (a single continuous narrative branch)
```

The goal is improvement, not perfection. A book-level organization is ideal, but a well-categorized graph is already highly useful.

## The Book View

In book view, sorted notes are rendered as a single continuous document. Each note is a discrete unit behind the scenes, but readers see a flowing manuscript.

For large screens, a **hybrid book-tree-graph view** places sorted notes as the main column, with unsorted notes branching off to the side — linked by shared categories. The side panel needs its own scroll, independent of the main book scroll. This view requires careful design given potentially many linked unsorted notes.

## Sorting Model

Sorting is a tree hierarchy built from **prior** relationships. Each note (or category) stores a reference to the note or category it follows.

- If a note's prior is a **category**, it is the first note of that section.
- If a note's prior is another **note**, it follows that note directly.
- Categories themselves have a prior (their parent category), forming a nested chapter structure.
- Categories should never have notes as a prior/parent.

### Prior Relationship Types

| Type | Behavior |
|---|---|
| `new_paragraph` | Standard paragraph break between the two notes (default) |
| `same_paragraph` | Notes flow into the same paragraph, space-separated |
| `indented_new_paragraph` | New paragraph indented one level; not recursive |
| `bullet_point` | Groups with siblings as a bulleted list; a bullet under a bullet creates a sub-list |
| `numbered_list` | Same as bullet but numbered |

Multiple notes sharing the same prior with `bullet_point` or `numbered_list` type are grouped as a list on one branch — the exception to the usual rule that multiple children create branching.

## Categories

Categories serve two purposes: linking notes to a topic, and acting as chapters/sections in book view.

Each category has:
- A **no-whitespace name** (used as a hashtag, e.g. `#electrical`)
- A **long format name** (used as a heading, can include whitespace)
- A **heading level** (H1–H6 or deeper) — stylistic weight, not hierarchy position; used as a tiebreaker between siblings with the same parent
- An optional **icon** for visual distinction (like book covers)

Categories can be added inline (`#category` in text) or in the note's metadata as a loose array. When typing a category, the UI autocompletes existing ones.

### Filtered Sorted View

In sorted view, users can filter by a secondary category. For example, house design notes sorted primarily by "trade" (electrical, framing) can be filtered down to show only notes tagged with a specific room (e.g. `#kitchen`), then each note can be opened to see it in full sorted context.

## Knowledge Packs

> **Status (Phase 6, implemented):** packs are a single distributable JSON file (`syllepsis-core::pack`);
> `app::pack` handles export by category, an import preview (per-note new/update/locally-modified status +
> category-mapping suggestions), and the import itself, which honors the `locally_modified` overwrite
> protection below. The React **Packs** view drives export and import.

Knowledge packs are curated collections of notes intended to be loaded into an existing book, as opposed to a standalone book.

Key behaviors:
- A note can belong to multiple knowledge packs.
- Categories are not explicitly part of a pack — they are pulled in by the notes that use them.
- Knowledge packs carry a **version number**.

### Import Flow

During import, a UI view allows:
- Mapping incoming categories to existing local categories (auto-suggests near-matches).
- Selective import — discard unwanted notes before committing.
- On version update: re-importing overwrites existing pack notes except those that were locally modified by the user.

### Export Flow

A UI view lets authors select notes for inclusion, typically by category. The export bundles selected notes and their metadata into a distributable pack.
