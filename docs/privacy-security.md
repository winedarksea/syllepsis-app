# Privacy & Security

## Centralized Policy View

A dedicated UI panel provides a single place to manage:
- Tagging notes or categories with any of the three **privacy capabilities** below (or the **Private** preset that sets all three; see [sync-backup.md](sync-backup.md))
- Setting files or categories as **locked**
- Viewing and adjusting access control across the book
- Generally we want to expose as many settings as possible to the user, to give them full control (but many of these somewhat hidden to prevent overwhelming the user).

## Privacy capabilities

Privacy is split into three **independent** capabilities, so a note can (say) stay out of the public publish while remaining locally searchable. Both notes and categories carry all three:

- **Hidden** — kept out of the main UI, default views, and exports. Still searchable and publishable unless also flagged otherwise.
- **Excluded from search** — left out of search and RAG retrieval. May still appear in default views.
- **Excluded from publish** — added to `.gitignore` and withheld from the static-site / GitHub publish. Still visible and searchable locally. Always included in the full Google Drive backup — this capability only governs the *public* release surface.

### The `private` preset

`private` is a convenience **preset**, not a separate flag: turning it on sets all three capabilities at once (hidden + excluded-from-search + excluded-from-publish), and turning it off clears all three. This is the one-click "make this fully private" action; the three capabilities remain individually toggleable for finer control.

### Legacy migration

Books written before the split stored a single `private: true` flag on a note's `lifecycle` or on a category. On load, that legacy flag is transparently expanded into all three capabilities (matching its old meaning) and the legacy key is never written back. No user action is required, and a legacy private note keeps behaving exactly as before. (Cloud-sync behavior is intentionally untouched by this migration — see [sync-backup.md](sync-backup.md).)

## Locked Files

Locked notes are not intended to be easily edited. Two locking modes:

### Unlock Delay
Users can add commentary or propose a rewrite, but must wait a configurable period (e.g. 24 hours) before the proposed change can be merged as the official text. This protects users from impulsive edits (e.g. a late-night session corrupting carefully written notes).

### Fact-Check Gate
A note can require the fact-check LLM call to return a passing grade (e.g. `strong_evidence`) before a proposed rewrite can be merged.

## Deletion Delay

Deleting or unlocking a note defaults to a 24-hour confirmation window. This is not about multi-user permissions — the assumption is one admin user — but about protecting users from themselves.

Deletion is implemented as "mark for deletion," with permanent removal after the configurable delay (default: 30 days for notes; runs on startup or user action). See [object-types.md](object-types.md#cleanup) for details.
