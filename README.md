<p align="center">
  <img src="frontend/public/favicon.svg" alt="Syllepsis" width="96">
</p>

# Syllepsis

An open-source, free, local-first note-taking app for building **books** — large, unified
knowledge spaces — aiming to organize understanding for large projects. Notes start as rough captures and
are progressively organized: uncategorized → graph → tree → continuous book. Everything is
plain markdown on your device, synced to your own cloud (Google Drive, GitHub). LLMs accelerate every step but are fully optional.

## Features

**Note types**
- Prose notes, quotes, references, to-dos, Q&A, tables, code blocks, pictures, drawings

**Organization**
- Hashtag categories with icons and display names
- Prior-relationship tree for hierarchical note ordering
- Notebox inbox — capture first, categorize later
- Book view — flatten the sorted tree into a continuous narrative document

**Search and discovery**
- Powerful search: three types of search (exact match, BM25, and RAG retrieval) fused to help you find all the relevant content for a query.
- Semantic graph: three visualization modes: categories, clustering, and timeline allow you to view the patterns of your notes to find and utilize the connections between your ideas.
- Related notes carousel: easily connect to the similar notes from the current note you are on, like the product recommendation system of an ecommerce site, but speeding up connections among your own ideas.
- Related notes , duplicate detection, and embedding coverage diagnostics

**AI and LLM**
- Summarize, expand, rewrite (style-guided), fix grammar, fact-check, and devil's advocate
- All LLM operations produce non-destructive proposals — accept or reject individually
- Custom style cards and editable prompts per task allow easy, repeatable formatting of ideas.
- Routes tasks to local ONNX models (Qwen3-0.6B, EmbeddingGemma 300M Q4) or cloud providers
- Local models are efficient and fully bundled, avoiding the usual hassle of Local LLM deployments

**Spatial worlds**
- Tag your notes to specific locations and view them on a map
- The default map is Earth, but you can also map onto imported images (floor plans, fantasy maps, memory palaces)
- `loc:` grammar resolves coordinates, named places, and CSV lookup tables at render time

**Privacy and lifecycle**
- Private, archived, locked, and mark-for-deletion states per note or category
- 24-hour timed unlock gate with optional fact-check requirement allow users to protect their most critical notes from accidental or impulsive updates.
- Managed `.gitignore` block automatically excludes private notes from git publishes

**Sync and storage**
- Plain markdown on disk — no proprietary database
- LWW-register CRDT with optional fine-grained Loro text CRDT
- Local folder sync by default; Google Drive and GitHub sync providers planned
- Conflict copies for binary assets; loop-prevention for re-syncs

**Export and publishing**
- Knowledge packs — portable versioned JSON bundles for sharing note collections
- Static site export — self-contained HTML with private content filtered out

**Extensibility**
- Sandboxed plugin system (Extism) allows users to develop customizations to the app
- Themes allow you to easily customize the colors and icons of the app for your own person look

See [`docs/`](docs/) for the full design and the implementation roadmap.

## Architecture

A Cargo workspace with a platform-agnostic Rust **core** and a thin Tauri shell; the React +
Lexical frontend talks to the core through typed Tauri command wrappers. Cloud sync is managed through OpenDAL, with Loro for CRDT. ONNX runtimes are used for local embedding and LLM models. 

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
