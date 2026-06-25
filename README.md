# Syllepsis

An open-source, local-first note-taking app for building **books** — large, unified
knowledge spaces — rather than quick notes or to-do lists. Notes start as rough captures and
are progressively organized: uncategorized → graph → tree → continuous book. Everything is
plain markdown on the user's device, synced to their own cloud (Google Drive, GitHub). There
is no hosted backend. LLMs accelerate every step but are fully optional.

See [`docs/`](docs/) for the full design and the implementation roadmap.

## Architecture

A Cargo workspace with a platform-agnostic Rust **core** and a thin Tauri shell; the React +
Lexical frontend talks to the core through typed Tauri command wrappers. The core has no Tauri or
UI dependency, so the future PWA/WASM build can reuse it behind the same trait seams
(`NoteStore`, `EmbeddingProvider`, `LlmProvider`, `CrdtBackend`, and `SyncProvider`).

```
crates/
  syllepsis-core/      platform-agnostic domain logic (notes, sorting, storage, markdown)
  syllepsis-tauri/     Tauri shell: #[tauri::command] wrappers + app state
frontend/              React + TypeScript + Vite
docs/                  design docs + implementation plan
```

### `syllepsis-core` modules

| Module | Responsibility |
|---|---|
| `id` | `{type}-{slug}-{ulid}` identity; monotonic ulids; resolution on the ulid tail |
| `model` | object types, full metadata schema, categories, prior/sort edges, worlds, style cards |
| `markdown` | frontmatter (de)serialization, the Syllepsis dialect (`#`, `@`, `loc:`, `%%`, `\|\|cloze\|\|`), pulldown-cmark wrapper |
| `storage` | book folder layout, the `NoteStore` seam + FS impl, id registry, the `Book` handle |
| `sort` | build the prior-relationship tree and flatten it into book view (+ markdown export) |
| `search` | hybrid BM25 + vector search with RRF fusion; embedding diagnostics; related-note carousel |
| `embeddings` | Versioned synced sidecars + SQLite projection + `EmbeddingProvider` seam + EmbeddingGemma 300M Q4 |
| `llm` | `LlmProvider` seam + offline heuristic + `OnnxLlmProvider` (Gemma 4 E2B, `onnx` feature); ChatML + proposal/cloud-handoff flow |
| `onnx` | Shared ONNX infrastructure: model manifest registry, file cache, sha256 verify, download planner, EP selection, session builder |
| `crdt` | `NoteCrdt`/`CrdtBackend` seams + always-on LWW-register backend + `LoroDocument` (fine-grained text CRDT, `loro` feature) |
| `sync` | `SyncProvider` seam + `LocalFolderSync` default; the sync engine (markdown ⇄ sidecar reconcile, plan/apply, conflict copies, loop prevention), per-device state, asset UUID sidecars |
| `spatial` | Worlds & overlays: the `loc:` grammar parser, the world registry (implicit `earth`), the CSV text→coordinate lookup table, coordinate resolution, and overlay (pins + regions) assembly over an image/geo world |
| `pack` | Knowledge packs: the portable, versioned [`Pack`] envelope (manifest + notes + categories) and its single-file JSON (de)serialization |
| `publish` | Read-only static-site rendering (markdown→HTML) and the idempotent managed-`.gitignore` block that excludes private content from a git publish |
| `app` | Framework-agnostic command surface (DTOs + operations) the Tauri shell wraps — including `lifecycle` (privacy/lock/deletion), `pack` (export/import), and `publish` (site + git exclusion) |
| `config` / `error` | Typed per-book config (no magic numbers) and the crate-wide error type |

## Status

**Phase 1 core complete and tested** (note model, markdown dialect, file storage, sort/book-render, application command layer).

**Phase 2 & 3 implementation is in place** (core/Tauri/frontend builds pass, `clippy -D warnings` clean):
shared ONNX Runtime infrastructure (model manifests, sha256-verified bundled/downloaded assets,
execution-provider selection, session builder) shared by the EmbeddingGemma 300M Q4 embedder (Phase 2) and the Gemma 4 E2B
local LLM (Phase 3). Canonical note vectors are persisted in synced `_embeddings/` sidecars and projected
into local SQLite; search degrades to exact + BM25 when query inference is unavailable. Real ONNX inference has gated ignored tests that require
`SYLLEPSIS_MODEL_CACHE` to point at a populated model cache.

**Phase 4 CRDT sync is in place** (`clippy -D warnings` clean, default suite green): per-note CRDT sidecars
behind the `NoteCrdt` / `CrdtBackend` seams (always-on deterministic LWW-register default; the fine-grained
[Loro](https://loro.dev) text CRDT behind the optional `loro` feature), the `SyncProvider` seam with a
`LocalFolderSync` default (a synced folder *is* how Drive/Dropbox desktop expose the cloud), and a sync engine
that reconciles markdown ⇄ sidecars, converges concurrent note edits, writes deterministic `.conflict-*` copies
for non-mergeable files, tracks binary assets by UUID sidecar, and is loop-safe (a quiet re-sync is a no-op).
Cloud HTTP providers (Google Drive, GitHub) are advertised in the provider registry but not yet wired.

**Phase 5 spatial worlds (first pass) is in place** (`clippy -D warnings` clean, default suite green): the
`loc:` location grammar (`lat,long`, `world/x,y`, `@named-place`, plain text) parsed and resolved against a
world registry (`earth` is the implicit default geo world; image-backed worlds reference a drawing/raster
backdrop) and a CSV text→coordinate lookup table carrying a `world` column. A note's frontmatter `location`
and inline `loc:` body tokens become overlay **pins**; a category's `location` becomes a pin, or a clickable
**region** when it also carries `SpatialRegion` geometry (an SVG element id, or a normalized bbox/polygon for
raster backdrops). The `app::spatial` command surface (list/create/delete worlds, build a world overlay,
read/edit the lookup table, resolve a token) is wrapped by Tauri commands, and the React **Worlds** view
renders pins and regions over a normalized coordinate plane (clicking a note pin opens the note; clicking a
category pin/region runs its filtered-sorted view). An image world's backdrop is served from the core as a
self-contained `data:` URL and drawn behind the overlay; geo map tiles remain a later pass.

**Phase 6 packs / privacy / serving is in place** (`clippy -D warnings` clean default + `--features loro`,
257 core tests / 260 with loro): the privacy & lifecycle behavior layer (`app::lifecycle`) finally acts on
the Phase-1 fields — private/archived/locked toggles, a delayed-deletion "mark for deletion" flow with a
scheduled purge (and self-destruct `vanish_at`), and the centralized policy overview — with private notes and
notes in private categories dropped from default views and RAG retrieval. Locked notes gate body changes: an
unlock delay holds a rewrite until the configured window elapses, and a fact-check gate requires a passing
check, enforced both in the LLM proposal-accept flow and on direct edits. Knowledge **packs** (`pack` +
`app::pack`) export a curated, versioned set of notes as a single distributable JSON file and import it with
category mapping and `locally_modified` overwrite protection on version re-imports. **Serving** (`publish` +
`app::publish`) renders a read-only static HTML site that excludes private content, and rewrites a managed
`.gitignore` block so a git publish never carries private notes/categories. The React **Privacy** and
**Packs** views drive all of it.

Next: cloud HTTP sync providers and git integration; sync UI; the geo map-tile view; plus remaining Phase 3
product UI work for keychain-backed cloud execution and richer LLM management.

## Developing

```sh
cargo test -p syllepsis-core               # run the core test suite (offline defaults)
cargo test -p syllepsis-core --features loro   # also exercise the fine-grained Loro CRDT backend
cargo clippy --all-targets -- -D warnings
(cd frontend && npm run build)
cargo fmt
```

## License

MIT — see [LICENSE](LICENSE).
