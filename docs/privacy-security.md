# Privacy & Security

## Centralized Policy View

A dedicated UI panel provides a single place to manage:
- Tagging categories as **private** (excluded from GitHub publish; see [sync-backup.md](sync-backup.md))
- Setting files or categories as **locked**
- Viewing and adjusting access control across the book

## Private Notes

Notes and categories can be tagged as private. Private content:
- Is excluded from the GitHub publish (via gitignore)
- Is included in the full Google Drive backup
- Does not appear in RAG retrieval or default views unless the user explicitly toggles them on

## Locked Files

Locked notes are not intended to be easily edited. Two locking modes:

### Unlock Delay
Users can add commentary or propose a rewrite, but must wait a configurable period (e.g. 24 hours) before the proposed change can be merged as the official text. This protects users from impulsive edits (e.g. a late-night session corrupting carefully written notes).

### Fact-Check Gate
A note can require the fact-check LLM call to return a passing grade (e.g. `strong_evidence`) before a proposed rewrite can be merged.

## Deletion Delay

Deleting or unlocking a note defaults to a 24-hour confirmation window. This is not about multi-user permissions — the assumption is one admin user — but about protecting users from themselves.

Deletion is implemented as "mark for deletion," with permanent removal after the configurable delay (default: 30 days for notes; runs on startup or user action). See [object-types.md](object-types.md#cleanup) for details.
