# TODO

# Big Items
There is no clear way to setup cloud sync from the UI for google drive. We also probably want to add some basic git commands to run from the UI (like select notes to include in a new commit).

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

Local LLM appears to be running CPU only, despite ONNX theoretically being able to use GPU on MacOS.
The status/progress overlay that should show up when the Local LLM is running is not showing up anywhere.

Device storage vs app storage for the app (like Obsidian, able to choose with more permissions for the book)

Edit mode, reading mode, source mode (raw = source)

Implement the "navigator's" theme alternative theme, and generally make sure theme switching is possible. Ideally each theme would support both light and dark mode. Also some special icons or even perhaps special visual style (such as different styles for the graph connections and nodes).

Need to make a GitHub Actions flow that publishes to syllepsis.org (our domain) a landing page as well as the built installer binaries available for download (likely all hosted on Cloudflare Pages).

A collection of related functions:
Unlock delay enforcement: 24-hour gate on unlocking files not enforced
Fact-check gate for locking: Can't require LLM verification before unlocking
Confirmation delays on delete: No 24-hour confirmation requirement before deletion
Fact-checking as a first-class workflow: Infrastructure exists but no dedicated fact-check command with the assertion "strong evidence | questionable | many issues | full failure" enum

We need a complete settings page with options like setting up cloud llm api tokens and changing theme

We likely need to revamp the "tools" dropdown, so a tool like 'generate summary' can be chosen to be local llm or cloud, to use mnemonic or acrostic summaries, and then have the progress bar clearly shown. It might make more sense as an opening dialogue of some kind.

No support for images yet.

Existing pack notes do not preserve prior. They should.

Embeddings aren't fully developed yet (like clusters). Should maybe have their own view, and link into statistics.

Merge note tool (LLM backed with preview and edit before final merge). Users should be able to specify the higher priority note (the one to try and save the most content from) of the two.
