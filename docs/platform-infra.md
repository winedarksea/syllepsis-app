# Platform & Infrastructure

## Target Platforms

| Platform | Delivery |
|---|---|
| macOS | DMG (Tauri) |
| Linux | Snapcraft (Tauri) |
| Windows | Microsoft Store (Tauri) |
| Android | Google Play (Tauri) |
| Web / Mobile | Progressive Web App (PWA) |

iOS is deferred (no Apple developer account currently).

## Tech Stack

| Layer | Technology |
|---|---|
| Frontend | TypeScript + [Lexical](https://lexical.dev/) |
| App core | Rust |
| Desktop / mobile shell | [Tauri](https://tauri.app/) + [Tauri-Specta](https://github.com/oscartbeaumont/tauri-specta) |
| Embeddings / vector store | [LanceDB](https://github.com/lancedb/lancedb) or fastembed-rs + sqlite-vec |
| ML (local) | [Candle](https://github.com/huggingface/candle) |
| CRDT sync | [autosurgeon](https://github.com/automerge/autosurgeon) or y-rs or [Loro](https://github.com/loro-dev/loro) |
| Markdown parsing | pulldown-cmark |
| Serialization | serde_yaml |
| Git integration | git2 |

Decision: Loro as the primary CRDT, with Automerge as the backup if we run into issues. Markdown is source of truth with CRDT sidecar, per note (perhaps with a book level registry as well).
For markdown edited externally of the CRDT, last edit timestamp should be checked to make sure this isn't an old change, then the set of diffs pulled in as single new CRDT update.
Decision: SQLite as the embedding backbone + sqlite-vec + FTS5, with Candle as the vector generation tool (with fastembed-rs as fallback plan). Put in a EmbeddingProvider trait so native and PWA can differ without the rest of the app caring. Embeddings are device-local.
Auth occurs via git or cloud drive access, no credentials are managed by this app. Git is not the main CRDT target, if present in addition to a cloud drive it is meant as a "public, partial rolling release" following behind the main edits.

### Potentially useful references
- https://github.com/manyougz/velotype — keyboard/text input patterns
- https://github.com/pop-os/cosmic-text — text rendering

## PWA Notes & Limitations

The PWA and Tauri builds share as much code as possible, but PWA has constraints:
- No git access (git integration is Tauri-only)
- Uses OPFS (Origin Private File System), which has weaker persistence in some environments
- Limited to WASM-enabled packages (`automerge-wasm`, `candle` WASM build, `ywasm`)

The PWA is the delivery path for web, iOS Safari, and Android without the Play Store.

## Codebase Philosophy

- **Small files, modular design.** Fewer lines of code so the initial POC is easier to adjust.
- **Clean seams around third-party libraries.** No library should be so deeply coupled that replacing it would require rewriting half the app — replacements should be hard but straightforward.
- **Good tests.** Users must trust they can't easily lose their insights. Sync, merge, and data integrity paths in particular need solid test coverage.
- **WYSIWYG markdown** is the long-term editing goal. The initial POC uses a simpler editing surface.

## Plugins & Themes

- **Plugins**: designed to be added in the future; must be sandboxed (e.g. WASM) for security.
- **Themes**: multiple supported from the start, including custom user themes. Default: light and dark mode.
- **Knowledge packs** (see [core-concepts.md](core-concepts.md)): downloadable collections of organized notes.
- **Long-term**: hosted marketplace for plugins, themes, and knowledge packs.

## Import / Export / Serving

- Import and export to allow data portability.
- A read-only server view option (see [ui-views.md](ui-views.md)) can be published as a website separately from the edit view.
- Export to book-like format for final manuscript production (intended for "large" idea spaces, not loose unrelated notes).
- Long-term: integration as "native" context to a user's primary LLM (e.g. Google Gemini via Drive, ChatGPT via plugin).

## Example Apps for Reference

LogSeq, Obsidian, Tana, Trilium, Plottr, Mem
