# Object Types

## Storage Format

Each note is an object. Most data is stored as a markdown text file. Metadata is stored in YAML frontmatter (hidden from the standard UI; surfaced through a dedicated metadata input area). Fenced blocks are an acceptable alternative to YAML frontmatter.

### Note IDs

Note IDs are auto-generated, decentralized (no hosted backend hands them out), and combine a human-readable element with a collision-proof element. The format avoids colons entirely so the same string is safe as both an ID and a filename on every platform (`:` is illegal in Windows filenames and special on macOS):

```
{type}-{slug}-{ulid}
```

- **`type`** — the storage-shape enum (`note`, `table`, `picture`, `drawing`, or internal `commentary`). Note subtypes such as quote and todo live in classification metadata.
- **`slug`** — derived from the title/content: ASCII-folded, lowercased, kebab-cased, stopwords trimmed, truncated to ~32 chars. **Cosmetic and mutable** — it may regenerate when the title changes.
- **`ulid`** — a [ULID](https://github.com/ulid/spec) (128 bits, Crockford base32, lowercased), the **canonical, immutable identity**. Time-ordered so files sort chronologically; high-entropy so independent devices generate IDs offline without colliding on merge, and so IDs stay unique even across imported knowledge packs.

Example: `note-montaigne-on-friendship-01jh5k3q2x9y8w7v6t5s4r3q2p`

**Identity rules:**
- The canonical `id` lives in **frontmatter**, never in the file path. The filename is a derived, disposable convenience. This is what lets a note move between [sorting subfolders](#storage-layout) or be renamed externally without losing identity.
- Links and lookups resolve on the **ulid tail**, so a stale slug still resolves correctly. Renaming a note's title is safe.
- **Forking** mints a new ulid (new identity) and records `forked_from` (see [Forking](#forking)). **Knowledge-pack re-import** matches on the existing id, so a stable, globally-unique id is what lets pack updates overwrite the right note.
- The [book-level registry](#book-level-metadata) indexes all ids and acts as a collision backstop: on creation, fork, and merge/pack-import, a new id is checked against the registry and its ulid regenerated on the astronomically rare hit.

The markdown dialect is versioned with a field like `markdown_version: syllepsis_001` so users viewing files outside the app can identify the format's origin.

`%%Your comment here%%` syntax adds comments that don't appear in rendered output.

`||Your spoiler here||` syntax hides text behind a click-to-reveal in rendered output. The same syntax doubles as a **cloze deletion** for study/learning: in a study mode the hidden spans become blanks the user recalls before revealing. An optional hint and group id are supported — `||hidden|hint||` shows the hint in place of the blank, and `||c1::hidden||` groups deletions that reveal together (multiple `c1` spans reveal as one). This ties into the [generative-learning goal](llm-ai-features.md#generative-learning-goal).

## Storage Layout

Each book is a **folder** on disk. Notes are markdown files within it, and book-level metadata lives in the same folder (see [Book-Level Metadata](#book-level-metadata)).

Subfolders are likely used to mirror sorting: a note sorted into a category in book view can be stored in a subfolder for that category, so the on-disk layout roughly tracks the narrative structure. This keeps the file tree navigable outside the app and plays well with file-based cloud sync (see [sync-backup.md](sync-backup.md)).

For each book:
_categories/ contains category frontmatter files
_commentary/ contains commentary
_cache/ or _derived/ for ephemerals, gitignored and possibly not cloud synced

## Storage Object Types

Object type is reserved for storage differences:
- **Notes**: Markdown body plus YAML frontmatter.
- **Tables**: CSV companion file plus YAML frontmatter.
- **Pictures**: imported raster asset plus Markdown/frontmatter metadata.
- **Drawings**: imported SVG asset plus Markdown/frontmatter metadata.
- **Commentary**: internal Markdown notes under `_commentary/`.

### Tables
Stored as CSV with YAML frontmatter. Special subtypes: **decision matrices** and **pro/con tables**.

### Pictures
Pictures are first-class note objects. Captions, description, categories, location, and the stable
asset reference live in the normal Markdown/frontmatter file. The imported binary is kept unchanged
under `assets/` with an adjacent UUID sidecar, so moving or renaming it does not break references.
Supported raster formats are PNG, JPEG, GIF, and WebP. XMP may be imported or exported later, but
it is not canonical storage.

Pictures do not have an archive option — only delete (implemented as "mark for deletion," then permanent removal after a configurable delay, default 30 days).

Crdt doesn't track images.

A raster image can serve as the backdrop for an [image-backed world](spatial-worlds.md#worlds) and carry an overlay of note/category pins and regions. Because raster images don't scale like vector, the overlay must apply an explicit zoom/pan transform so pins stay anchored to the correct spot as the user zooms — SVG [drawings](#drawings) are preferred where clean zoom or named regions matter.

### Drawings (SVG)
An object type for vector drawings, stored as imported **SVG** plus the same Markdown metadata and
UUID sidecar used by pictures. A leading XML/DTD prolog is stripped before storage so DTD-bearing
SVGs can be parsed without enabling DTD processing. Imported SVG is validated before ingestion:
scripts, active foreign content, event handlers, and external references are rejected, while safe
element IDs are preserved for world regions.

**Imported SVGs are treated as drawings** — there is no separate "imported vector image" type. The future in-app drawing tool will also emit SVG, so a hand-drawn graphic and an imported one are handled identically.

Drawings are the **preferred backdrop for image-backed worlds** (floorplans, mind palaces): being vector, they zoom cleanly, and a named element (`id="kitchen"`) doubles as a clickable overlay region. See [spatial-worlds.md](spatial-worlds.md) for overlays and [sync-backup.md](sync-backup.md#drawings-and-svg) for how drawing geometry is synced (SVG is text, but not CRDT-tracked by default).

## Text Classifications

### Summary / Description Duality

Every text object has two views of the same concept:
- **Summary**: short, like a flashcard front or chapter blurb
- **Full description**: the main body (paragraphs, details)

Users can view a category in summary-only mode (cards), then click to expand to the full description. LLMs can generate one from the other (using a style card and metadata as guides).

A warning displays if:
- The summary exceeds 250 characters, or
- The summary is longer than 10% of the full description (whichever is larger)

A metric shows the summary-to-full-description ratio alongside a vector similarity score to keep them in alignment.

Text notes can be classified as `note`, `qa`, `reference`, `quote`, `code`, `todo`, `idea`,
`hypothesis`, `factual_claim`, `rule_or_requirement`, `principle`, `preference`, `procedure`,
`context`, `analysis_or_interpretation`, or `narrative`. The default is `note`.

Creating a note from a subtype shortcut seeds an editable Markdown hint, but the app does not
enforce that format yet:
- **Todo**: `- [ ] `
- **Q&A**: `question:` / `answer:` body fields.
- **Quote**: blockquote plus `Source:`.
- **Reference**: lightweight citation line.
- **Code**: fenced `text` code block.

### Quotes
Text plus a source. Signals that the content was written by someone else. Linked reference uses `@`.

### QA (Question & Answer)
Starts with editable `question:` and `answer:` body fields. Later versions may enforce a stricter
shape, but for now it is a hint only.

### References
Author, Year, Title, URL (shown on hover). Tagged with `@`.
- Year = publication year (accessed year not tracked separately).
- References have no summary and are expected to be mostly fixed metadata.

### Todos
A note classification optimized for checklist items. Includes syntax sugar and auto-archiving:
done/cancelled items move to a todo archive file after a configurable number of days (with
`completed:date` added).

**Status syntax:**
```
- [ ]  open (not started)
- [/]  active
- [?]  needs_clarification
- [>]  deferred
- [-]  cancelled
- [x]  done
```

**Date syntax** (typing `due:` opens a calendar widget):
```
due:2026-01-01
start:2026-01-01
done:2026-01-01
```

**Priority:** `p:0` `p:1` `p:2` `p:3`

**Task linking:**
```
taskid:<user-name>       — assigns a linkable ID to a line
waiting:taskid           — this task is waiting on another
after:taskid             — do not start until taskid is done
blocked-by:taskid        — blocked
```

`#` and `@` work for categories and reference links in all text notes.

### Commentary
A special type linked by ID to a specific note. Used for AI proposals, fact checks, and proposed rewrites until they are accepted or discarded.

Commentary is searchable but not shown in standard views until the user drills into the linked note.

**Commentary metadata includes:**
- When generated and by whom
- A **status enum** (e.g. for fact checks: `strong_evidence`, `some_questionable_points`, `many_questionable_points`, `full_failure`; for writing quality: `needs_rewrite`, `minor_issues`)

When a rewrite is accepted, the commentary replaces the original note body. The user can optionally move the old version to a commentary node ("store old version as commentary").

LLM response types are an extensible family: fact checks, devil's advocate (seeks potential flaws), grammar/style checks, etc.

### Future Text Classifications
- **Executable code cells**: code isolated in a WASM sandbox (distinct from the display-only code blocks above).
- **Query cells**: dataview-style query cells that compute their content from other notes' metadata ([Obsidian Dataview](https://blacksmithgu.github.io/obsidian-dataview/) is the reference for the idea).
- **Worksheet**: a structured fill-in object type (details TBD).

## Metadata Fields

### Date Metadata
- **Tracked automatically**: creation date, last update date
- **User-optional**: scheduled/target date, completion date
- Dates can be expressed as `+N days` relative to another note's date
- Reminder flag can be added to any date
- **Future**: import/export of dates to/from an external calendar; see the [Timeline view](ui-views.md#timeline-view-future)

### Location Metadata
Any object can link a location. The simplest form is a plain text string resolved through a CSV lookup table that maps text → coordinates. The lookup table carries a `world` field (default: Earth; designed to support fantasy maps, other planets, or image-backed planes like floorplans).

Beyond the plain-text form, location is a first-class spatial concept:
- **Inline `loc:` syntax** places a coordinate mid-note or in a table cell (e.g. `loc:47.6062,-122.3321` for Earth, `loc:firstfloor/0.42,0.31` for an image-backed world). Typing `loc:` opens a location picker, mirroring how `due:` opens a calendar.
- **Note-level `location`** in frontmatter pins an entire note to a coordinate.
- A coordinate's **world** can be real Earth lat/long or a generic world — and a world can just be an image (e.g. a floorplan), letting notes be tagged onto a drawing or photo.

See [spatial-worlds.md](spatial-worlds.md) for the full model (worlds, overlays, the `loc:` grammar, and the future map view).

### Authorship
Lightweight multi-author tracking:
- `created_by`, `edited_by` (array), `ownership` (manually assignable)
- Ties to cloud sync identity provider (GitHub, Google) rather than local user management
- Supports an alias for a friendlier display name
- Tracks AI vs. human authorship

### Forking
Notes can be forked (duplicated). A forked note stores:
- `forked_from`: parent note ID
- `forked_at`: timestamp
- `ownership`: updates to the forking author

## Text Object Metadata Schema (draft)

```json
{
  "kind": "note | qa | reference | quote | code | todo | idea | hypothesis | factual_claim | rule_or_requirement | principle | preference | procedure | context | analysis_or_interpretation | narrative",
  "basis": "science_and_data | regulation_or_standard | logic_and_reasoning | tradition_and_culture | established_lore_or_fiction | lived_experience | personal_preference | none",
  "checkability": "objectively_checkable | partly_judgment_based | subjective_or_personal | none",
  "stability": "settled | evolving | tentative",
  "priority": "standard | important | core",
  "starred": "true | false",
  "stylistic_elements": ["anecdote", "metaphor"]
}
```

## Kanban / Scrum Metadata

Fields like `assignee` and `magnitude` are included for completeness to support todo/kanban use cases, but are a lower-priority secondary feature.

## Cleanup

- **Archive**: hides notes from RAG and default views; togglable. Not for pictures (those are deleted, not archived, with a delete confirmation dialogue).
- **Vanishing notes**: self-delete after a configurable number of days (default 180), set at creation.
- **Deletion**: "mark for deletion" → permanent removal after configurable delay (default 30 days). Runs on startup or user action; no need for exact timing.

## Book-Level Metadata

Stored as a markdown file alongside notes. Includes:
- Preferred language
- Book name
- Location (e.g. city of construction, so LLMs can look up local building codes)
- Cover image/icon (SVG, JPG, PNG; future: drawing interface)
