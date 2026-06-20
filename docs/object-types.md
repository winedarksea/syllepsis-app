# Object Types

## Storage Format

Each note is an object. Most data is stored as a markdown text file. Metadata is stored in YAML frontmatter (hidden from the standard UI; surfaced through a dedicated metadata input area). Fenced blocks are an acceptable alternative to YAML frontmatter.

Note IDs are auto-generated but aim to add human-readable elements, e.g.:
- `quote:author-textsource`
- `reference:author-source`

The markdown dialect is versioned with a field like `markdown_version: syllepsis_001` so users viewing files outside the app can identify the format's origin.

`%%Your comment here%%` syntax adds comments that don't appear in rendered output.

## Storage Layout

Each book is a **folder** on disk. Notes are markdown files within it, and book-level metadata lives in the same folder (see [Book-Level Metadata](#book-level-metadata)).

Subfolders are likely used to mirror sorting: a note sorted into a category in book view can be stored in a subfolder for that category, so the on-disk layout roughly tracks the narrative structure. This keeps the file tree navigable outside the app and plays well with file-based cloud sync (see [sync-backup.md](sync-backup.md)).

For each book:
_categories/ contains category frontmatter files
_commentary/ contains commentary
_cache/ or _derived/ for ephemerals, gitignored and possibly not cloud synced

## Special Non-Text Types

### Tables
Stored as CSV with YAML frontmatter. Special subtypes: **decision matrices** and **pro/con tables**.

### Pictures
Captions and other metadata are stored in XMP metadata using the same markdown format as text notes. Supported formats: PNG, JPEG, GIF, SVG, WebP.

Pictures do not have an archive option — only delete (implemented as "mark for deletion," then permanent removal after a configurable delay, default 30 days).

Crdt doesn't track images.

### Code Blocks
A special text type. **Mermaid** diagrams (including Venn diagrams) are a special subtype that can render inline.

### Drawings (future)
A future object type for freehand/vector drawings, with built-in render-to-image so they can be embedded wherever a static image is expected. Related to the future drawing interface for book covers.

## Text Object Types

### Summary / Description Duality

Every text object has two views of the same concept:
- **Summary**: short, like a flashcard front or chapter blurb
- **Full description**: the main body (paragraphs, details)

Users can view a category in summary-only mode (cards), then click to expand to the full description. LLMs can generate one from the other (using a style card and metadata as guides).

A warning displays if:
- The summary exceeds 250 characters, or
- The summary is longer than 10% of the full description (whichever is larger)

A metric shows the summary-to-full-description ratio alongside a vector similarity score to keep them in alignment.

### Quotes
Text plus a reference. Signals that the content was written by someone else. Linked reference uses `@`.

### QA (Question & Answer)
Renames "summary" → "question" and "description" → "answer". Both are shown simultaneously rather than one at a time. If the answer is a single link, the question acts as a tag pointer to that section.

### References
Author, Year, Title, URL (shown on hover). Tagged with `@`.
- Year = publication year (accessed year not tracked separately).
- References have no summary and are expected to be mostly fixed metadata.

### Todos
A special text type containing only checklist items. Includes syntax sugar and auto-archiving: done/cancelled items move to a todo archive file after a configurable number of days (with `completed:date` added).

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

### Future Text Object Types
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
Any object can link a location as a plain text string. A separate CSV lookup table maps text → lat/long for map views. The lookup table references a "world" field (default: Earth; designed to support fantasy maps or other planets in the future).

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
  "statement_type": "hypothesis | factual_claim | rule_or_requirement | principle | preference | procedure | context | analysis_or_interpretation | narrative | idea",
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

- **Archive**: hides notes from RAG and default views; togglable. Not for pictures (those are deleted, not archived).
- **Vanishing notes**: self-delete after a configurable number of days (default 180), set at creation.
- **Deletion**: "mark for deletion" → permanent removal after configurable delay (default 30 days). Runs on startup or user action; no need for exact timing.

## Book-Level Metadata

Stored as a markdown file alongside notes. Includes:
- Preferred language
- Book name
- Location (e.g. city of construction, so LLMs can look up local building codes)
- Cover image/icon (SVG, JPG, PNG; future: drawing interface)
