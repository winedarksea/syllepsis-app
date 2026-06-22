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
| Desktop / mobile shell | [Tauri](https://tauri.app/) command shell; generated bindings remain planned |
| Vector store | SQLite + FTS5 (BM25 / sparse) now; `sqlite-vec` remains the intended dense acceleration layer once the Rust binding is reliable |
| ML — embeddings (local) | ONNX Runtime (`ort`) native; `onnxruntime-web` / Transformers.js in the PWA. Default model: [Qwen3-Embedding-0.6B](https://huggingface.co/onnx-community/Qwen3-Embedding-0.6B-ONNX) |
| ML — local LLM | [ONNX Runtime](https://github.com/pykeio/ort) (`ort`) native; `onnxruntime-web` / [Transformers.js](https://github.com/huggingface/transformers.js) in the PWA |
| Cloud LLM | Rust router + Tauri keychain-backed HTTP execution; OpenAI-compatible + Anthropic |
| CRDT sync | [Loro](https://github.com/loro-dev/loro) (primary, implemented behind the `loro` feature); LWW-register default always compiled; Automerge as backup |
| Markdown parsing | pulldown-cmark |
| Serialization | serde_yaml |
| Git integration | git2 |

Decision: Loro as the primary CRDT, with Automerge as the backup if we run into issues. Markdown is source of truth with CRDT sidecar, per note (perhaps with a book level registry as well).
For markdown edited externally of the CRDT, last edit timestamp should be checked to make sure this isn't an old change, then the set of diffs pulled in as single new CRDT update.
Rust backend manages the CRDT, with Lexical not managing the CRDT.
Decision: SQLite as the embedding backbone + sqlite-vec (dense) + FTS5 (BM25/sparse), with embeddings generated on the **same ONNX Runtime stack as the LLM** — `ort` native on desktop, `onnxruntime-web` / Transformers.js in the PWA. Bundled embedding model is **Qwen3-Embedding-0.6B** (1024-dim, 32k context, Matryoshka-truncatable, asymmetric query/document); it shares the model-manifest + first-run-download + execution-provider-selection infrastructure with the bundled LLM. The EmbeddingProvider trait keeps native and PWA interchangeable without the rest of the app caring. Embeddings are device-local.
Sync auth occurs via git or cloud drive access; cloud LLM API keys are separate and belong in the OS keychain, never synced config. Git is not the main CRDT target, if present in addition to a cloud drive it is meant as a "public, partial rolling release" following behind the main edits.
Decision: ONNX Runtime is the local ML runtime for both the bundled LLM and the bundled embedding model. The bundled LLM is **Gemma 4 E2B IT, Q4 quantized** using the split text path (`embed_tokens_q4.onnx` + `decoder_model_merged_q4.onnx` plus external data files); the bundled embedder is **Qwen3-Embedding-0.6B, Int8** (`model_quantized.onnx`). Both share one download/cache/verify/EP-selection infrastructure behind the `onnx` Cargo feature (config-driven model manifests, sha256-verified first-run download, session builder). Burn and Candle were evaluated and rejected: the `onnx-community` exports depend on ORT *contrib* ops (`MatMulNBits`, `GroupQueryAttention`, `RotaryEmbedding`) that standard-ONNX importers cannot ingest. ORT is the only native consumer. The same model files run in the PWA via `onnxruntime-web` / Transformers.js with WebGPU. Everything sits behind an `LlmProvider` trait (and the parallel `EmbeddingProvider` trait) so native/PWA/cloud differ without the rest of the app caring.
Decision: cloud LLM calls execute in the desktop shell — Rust owns routing, prompt-building, provider HTTP calls, and the proposal/accept flow. API keys live in the OS keychain and are never written to synced config/markdown or returned to the frontend (consistent with the no-credentials principle above). OpenAI-compatible providers use a configurable base URL, so a local llama.cpp/Ollama/LM Studio server can be used without an API key.

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
