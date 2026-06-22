# TODO
In "create new book" the "Location" is a bit confusing (users might think that is file folder location) and it might help if it suggested a format.

When a user creates a new note, it isn't clear that the giant black canvas is where the note should go (the prompt only points at the summary).

There is no way to go to the Metadata for the note (the rest of the frontmatter, which is optional, like adding a location or dates).
Related issue: There is no way to take unsorted notes and sort them.

We have no text editing tools, so when I copy and pasted something that had after-paragraph spacing, I have no way of changing it.

There is no clear way to setup cloud sync from the UI for google drive. We also probably want to add some basic git commands to run from the UI (like select notes to include in a new commit).

I added a category to a note, but it still says “No categories yet” in the categories section. Refresh of that doesn't seem to happen.

Auto-save might be nice, if it can be done elegantly. It looks like I lose content just by switching out of the app sometimes, which is a problem (losing on closing makes some sense, but not losing on just switching out).

It would be nice if “fancier” tools like LLM calls (and maybe searches, etc) logged to the console when running in `tauri dev` mode

In the menu bar, under “Edit” -> “Writing Tools” I see what I think are Mac’s built in Apple Intelligence writing tools (like a “summarize”) option. While it is nice to see these, it seems because it auto-detected a text field, I also want to see buttons somewhere for our own LLM tools, like those for automatically generated the summary metadata based on the text of the note.

I don’t see any option for switching from one book to another, or otherwise going backing to the launch screen without closing the app.

Diagnostics say “no duplicates found” which is not surprising, I only have 2 test notes, but there is no clear way there for users to know when those checks were last run, or to trigger them themselvess

“New Note” exists but I see no way to add an image or table or other type of item (and add their corresponding metadata).


### Cloud sync from the UI (Google Drive)
Backend has a working `LocalFolderSync` + `SyncEngine`; `google_drive` and `github` providers are
*declared but unimplemented*. No sync UI exists yet.

| Option | Pros | Cons |
|---|---|---|
| **A. Mounted-folder UI on existing LocalFolderSync** (user points at their Google Drive desktop-app folder) | Reuses fully-working code; no OAuth/secret storage; ships fast; provider-agnostic (works with Dropbox/iCloud/USB too) | Requires the Drive desktop app installed + folder synced; sync timing depends on Drive's own client; no per-book Drive auth |
| **B. Native Google Drive API (`google_drive` provider + OAuth)** | True cloud sync without a desktop client; can show real sync status; mobile-friendly later | Large effort (OAuth flow, token refresh, secret storage, Drive API paging, conflict handling against `SyncEngine`); Google verification/app review; ongoing API maintenance |

Recommendation if/when picked up: **A first** (small UI over existing engine), keep **B** as a later
provider implementation behind the already-declared `google_drive` descriptor.

### Basic git commands from the UI
No git execution exists today — only `.gitignore` management (`refresh_private_gitignore`). The user
wants to e.g. select notes and make a commit.

| Option | Pros | Cons |
|---|---|---|
| **A. Shell out to system `git`** (new Tauri commands: status/add/commit/push) | Minimal code; no new heavy dependency; matches user's existing git workflow; easy to support push/remotes | Requires git installed + on PATH; must parse porcelain output; surface auth/credential errors cleanly |
| **B. Bundle `git2` (libgit2)** | Works without a system git install; structured API (no output parsing) | Heavy build dependency; libssh/HTTPS auth is fiddly; more code to cover status/stage/commit/push |
| **C. Defer** | — | Feature absent |

Recommendation if/when picked up: **A (shell out)** — a `git_status`/`git_commit(paths, message)`
command set, with a UI that lists changed note files (mapping note titles → `.md` paths) and lets the
user pick which to stage. Reuse `.gitignore` management already in place.


When a user clicks "details and metadata" it goes to an all white screen

Notes are always saved as "note-new-note-xxx" which is because New Note is the default title. It might be nice to save as the actual note title once the user first initiates a save (this might conflict with auto-save slightly, since it will often take them a bit to finish the title).

The UI has no option to delete a note.

When a user deletes a #category from the body text it doesn't delete the category from the note. This might be fine, but could sometimes be confusing.

Each of the created note types has identical body inputs. A table should have a basic table interface like a spreadsheet. To-Do should start with checkboxes. Code should start as a code block.

It looks like the AI tools are failing locally (it should be using Gemma 4 E2B by default). When I click "generate summary" it seems to just provide the truncated start of the body back as the summary, with the following logs:
```
2026-06-22T19:25:17.823142Z  INFO syllepsis_core::llm::service: llm: generating proposal task="summarize" provider="offline" model=gemma-4-e2b live=false note=note-new-note-01kvr95z8esbfbx5m32q87m5et
2026-06-22T19:25:17.823265Z  INFO syllepsis_core::llm::service: llm: proposal ready task="summarize" provider="offline" elapsed_ms=0 chars=62
```
