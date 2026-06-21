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
| Vector store | SQLite + `sqlite-vec` (dense) + FTS5 (BM25 / sparse) |
| ML — embeddings (local) | [fastembed-rs](https://github.com/Anush008/fastembed-rs) (built on `ort`) native; `onnxruntime-web` / Transformers.js in the PWA. Default model bge-m3; [Qwen3-Embedding-0.6B](https://huggingface.co/onnx-community/Qwen3-Embedding-0.6B-ONNX) as a manifest-swappable alternative |
| ML — local LLM | [ONNX Runtime](https://github.com/pykeio/ort) (`ort`) native; `onnxruntime-web` / [Transformers.js](https://github.com/huggingface/transformers.js) in the PWA |
| Cloud LLM | Rust router → [Vercel AI SDK](https://sdk.vercel.ai/) (frontend execution); OpenAI-compatible + Anthropic |
| CRDT sync | [autosurgeon](https://github.com/automerge/autosurgeon) or y-rs or [Loro](https://github.com/loro-dev/loro) |
| Markdown parsing | pulldown-cmark |
| Serialization | serde_yaml |
| Git integration | git2 |

Decision: Loro as the primary CRDT, with Automerge as the backup if we run into issues. Markdown is source of truth with CRDT sidecar, per note (perhaps with a book level registry as well).
For markdown edited externally of the CRDT, last edit timestamp should be checked to make sure this isn't an old change, then the set of diffs pulled in as single new CRDT update.
Rust backend manages the CRDT, with Lexical not managing the CRDT.
Decision: SQLite as the embedding backbone + sqlite-vec (dense) + FTS5 (BM25/sparse), with embeddings generated on the **same ONNX Runtime stack as the LLM** — fastembed-rs (built on `ort`) native, `onnxruntime-web` / Transformers.js in the PWA. This drops Candle entirely: it had been the embedding generator, but once the LLM forced ORT in, keeping a second ML runtime for embeddings bought nothing. Default embedding model is bge-m3 (dense, ~8192-token context, 1024-dim) with Qwen3-Embedding-0.6B as a manifest-swappable alternative; both ship via the same model-manifest + first-run-download machinery as the LLM. The EmbeddingProvider trait keeps native and PWA interchangeable without the rest of the app caring. Embeddings are device-local.
Auth occurs via git or cloud drive access, no credentials are managed by this app. Git is not the main CRDT target, if present in addition to a cloud drive it is meant as a "public, partial rolling release" following behind the main edits.
Decision: ONNX Runtime is the local-LLM runtime (the bundled model is Gemma 4 E2B, 4-bit). Burn and Candle were evaluated and rejected *for the LLM*: the `onnx-community` Gemma 4 E2B export depends on ONNX Runtime *contrib* ops — `MatMulNBits` (int4), `GroupQueryAttention` (carries the KV-cache), `RotaryEmbedding` — which are not standard ONNX, so `burn-import` (standard `ai.onnx` only) cannot ingest it. ORT is the native consumer; the same artifact runs in the PWA via `onnxruntime-web` / Transformers.js with WebGPU. Embeddings run on this same ORT stack (fastembed-rs native, `onnxruntime-web` / Transformers.js in the PWA), so the app ships a single ML runtime end-to-end and Candle is dropped. Everything sits behind an `LlmProvider` trait (and the parallel `EmbeddingProvider` trait) so native/PWA/cloud differ without the rest of the app caring.
Decision: cloud LLM calls are hybrid — Rust owns routing, prompt-building, and the proposal/accept flow; the frontend executes the call via the Vercel AI SDK (streaming + structured output) and posts the result back for Rust to wrap as a proposal. API keys live in the OS keychain and are never written to synced config/markdown (consistent with the no-credentials principle above).

### Potentially useful references
- https://github.com/manyougz/velotype — keyboard/text input patterns
- https://github.com/pop-os/cosmic-text — text rendering

## PWA Notes & Limitations

The PWA and Tauri builds share as much code as possible, but PWA has constraints:
- No git access (git integration is Tauri-only)
- Uses OPFS (Origin Private File System), which has weaker persistence in some environments
- Limited to WASM-enabled packages: `loro-wasm` / `automerge-wasm` for CRDT, and `onnxruntime-web` / Transformers.js, which runs **both** the local LLM and the embedding model — one ML library, no separate embeddings WASM build

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
