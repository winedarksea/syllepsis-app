# LLM & AI Features

LLMs are widely integrated but **fully optional**. All core functionality works without an API key.

## Cloud LLM Uses

- **Fact checking**: throw out a hypothesis, then have an LLM (prompted for groundedness) assess what is scientifically known. Fact check results are stored as [Commentary](object-types.md#commentary) nodes with a status enum (`strong_evidence`, `some_questionable_points`, `many_questionable_points`, `full_failure`).
- **Devil's advocate**: a prompt type that specifically seeks potential flaws in an argument.
- **Summarization / description generation**: generate a summary from full text or expand a summary into full paragraphs (guided by a style card and metadata). A generic "create summary" button is the default; summary **format options** let the user instead prompt for a mnemonic form — an **acronym** (a memorable word/initialism from key points) or an **acrostic** (lines whose first letters spell a word). These are mnemonic aids for the [generative-learning goal](#generative-learning-goal) and pair naturally with cloze-deletion study (see [spoilers & cloze](object-types.md#storage-format)).
- **Writing quality / grammar check**: stored as commentary with a status like `needs_rewrite` or `minor_issues`.
- **Category suggestion**: combine clustering results with an LLM call to suggest new or refactored categories.
- **Notes as LLM context**: surface selected notes to LLMs so users can ask questions grounded in their own knowledge base.

LLM response types are an extensible family — new prompt types can be added without changing the core commentary architecture.

### Proposal Flow
When an LLM rewrites a non-empty section, a clear accept/reject proposal is shown before any overwrite. Users can enable auto-accept upfront. Rejected rewrites are discarded; accepted rewrites replace the original (with an option to archive the old version as a commentary node).

## Local Embeddings

Embeddings run locally on the **same ONNX Runtime stack as the bundled LLM** (see [Local LLM](#local-llm-bundled)):
[`fastembed-rs`](https://github.com/Anush008/fastembed-rs) — which is built on `ort` and bundles
tokenization + pooling — on desktop, and `onnxruntime-web` / Transformers.js with WebGPU in the PWA.
This means **one ML runtime end-to-end, not two**: Candle is dropped. It had been kept only to generate
embeddings, but once the LLM forced ONNX Runtime in, a second ML stack bought nothing — and the embedding
model now reuses the LLM's **config-driven model manifest, sha256 first-run download, execution-provider
selection, and diagnostics** (embedding models are small relative to the 3.5 GB LLM, so this is nearly free).

The default model is **[BAAI/bge-m3](https://huggingface.co/BAAI/bge-m3)** via its
[ONNX export](https://huggingface.co/Xenova/bge-m3): 1024-dim, ~8192-token context, multilingual, and
*symmetric* (queries and documents embed the same way, no instruction prefix). A lighter bge variant is the
first-pass test model. **[Qwen3-Embedding-0.6B](https://huggingface.co/onnx-community/Qwen3-Embedding-0.6B-ONNX)**
is a manifest-swappable alternative — higher retrieval quality, 32k context, and Matryoshka dimensions that
can be truncated for cheaper storage/prefilter — at the cost of instruction-prefixed, *asymmetric* query vs.
document embedding. Swapping or adding an embedding model is a manifest entry, exactly like the LLM.

The model is used purely as a **dense** embedder. The sparse / BM25 arm of search comes from SQLite **FTS5**
([search.md](search.md)), not the embedding model, so a model with no native sparse output (Qwen3) loses
nothing, and bge-m3's learned-sparse head is an optional bonus the pipeline does not depend on. Vectors are
stored in SQLite via `sqlite-vec`, with the column width pinned to the active model's dimension.

### Multiple Vectors Per Note
- One vector for the **summary**
- One or more vectors for the **main body**
- Long notes are chunked for retrieval granularity — chunk size is a configurable tuning knob, not a model
  limit (bge-m3 handles ~8192 tokens) — and each chunk gets its own vector, tokenized with the model's own tokenizer

### Category Vectors
A category's vector is computed as an average of its member notes' vectors. This enables category-to-category similarity (duplicate/near-duplicate category detection) and the category upweighting used in the [related carousel](ui-views.md#related-carousel).

### Uses of Embeddings

| Feature | Description |
|---|---|
| **Vector search** | Semantic search across all notes |
| **Clustering** | Suggest new or reorganized categories (also using nearest neighbors to help suggest organization) |
| **Coherence analysis** | Detect consistency or narrative flow issues |
| **Duplication detection** | Surface most-similar notes and most-similar categories |
| **Blind spot detection** | Sort by *reverse* similarity to neighbors — lowest scores suggest disconnected narrative or missing content |
| **Style comparison** | Compare a note's style vector against style card vectors |

## Local LLM (bundled)

Goal: package a small-but-capable model so LLM features **just work** on the user's device with no
configuration. The bundled model is **Gemma 4 E2B, 4-bit quantized** (the choice as of mid 2026; a
future Gemma replaces it as a manifest edit, not a code change). Users with capable machines can opt
into a larger model (e.g. the 12B 4-bit), gated behind a RAM check.

**Runtime: ONNX Runtime.** The bundled model runs through ONNX Runtime — the [`ort`](https://github.com/pykeio/ort)
crate on desktop (with CoreML / DirectML / CUDA execution providers and a CPU fallback), and
`onnxruntime-web` / [Transformers.js](https://github.com/huggingface/transformers.js) with WebGPU in
the PWA. Both run the *same* model files and sit behind the `LlmProvider` seam, so the rest of the app
is runtime-agnostic. (Burn and Candle were evaluated and rejected *for the LLM*: the `onnx-community`
Gemma 4 E2B export depends on ORT *contrib* ops — `MatMulNBits` int4, `GroupQueryAttention` which
carries the KV-cache, and `RotaryEmbedding` — that standard-ONNX importers like `burn-import` cannot
ingest. Candle still does local embeddings.)

**Packaging.** The model is **not** shipped in the installer (~3.5 GB for the q4 text path = token
embeddings + decoder). It is a sha256-verified **first-run download** to an OS app-data models
directory shared across books, driven by a config-driven model manifest (id, repo, files,
quantization, hash, size, min-RAM, required execution providers). Replacing Gemma with its successor,
or adding the optional 12B, is a manifest entry. ORT picks the best available execution provider with
a CPU fallback and records which it used (for Diagnostics).

## Providers & Routing

LLM work is gated behind the `LlmProvider` seam, with three kinds of provider:
- **Local** — the bundled Gemma via ONNX Runtime (above).
- **Cloud** — hybrid execution: Rust owns the router, prompt-building, and the proposal/accept flow,
  while the frontend runs the actual call via the [Vercel AI SDK](https://sdk.vercel.ai/) (streaming +
  structured output, which replaces hand-parsing of category/status replies) and posts the result back
  for Rust to wrap as a proposal.
- **Bring-your-own** — any OpenAI-compatible endpoint (llama.cpp server, Ollama, LM Studio, most
  clouds) via a base URL; Anthropic for Claude.

**Keys** live in the OS keychain and are never written to synced config or markdown — consistent with
"no credentials are managed by this app" ([platform-infra.md](platform-infra.md)).

Users configure **per-task routing**: each task names a `{provider, model}` pair, so (for example)
summaries run on the local model while fact-checks run on a cloud Opus model. A per-action override
lets the user pick a stronger model for a single call.

**Long-term:** native platform LLMs (Windows AI API, Apple Intelligence, LiteRT-LM) for simpler tasks
where they suffice (proofreading, summarization; OCR via WASM / WebNN).

## Google / Gemini Integration
Notes synced to Google Drive (as backup) may automatically surface as Gemini context — no additional integration required.

## Style Cards

Style cards capture the writing style of a corpus so LLMs can generate or rewrite text in that style.

### Creation Workflow
1. Provide a corpus of text.
2. Generate embedding vectors for the corpus.
3. Discover **exemplars** using embeddings — the top 5 sentences (1–3 sentences each) most emblematic of the style.
4. Pass exemplar pieces to an LLM to produce a first draft of the style enum and description.
5. Human review and finalization.

Style cards are versioned to support future attribute updates. Each card optionally links the embedding model used (stored as key-value pairs of vectors per model, to support multiple embedding models).

URLs to openly accessible source texts can be included (e.g. Shakespeare sonnets for a sonnets style card).

### Style Card Schema (draft)

```yaml
---
short_description: a sentence or two describing the style in freeform
field: technical | instructional | persuasive | narrative | reflective | administrative
tenor: intimate | peer | expert_to_peer | expert_to_novice | institutional
mode: spoken | conversational_written | edited_written | formal_written
density: sparse | moderate | dense
texture: plain | polished | vivid | aphoristic | procedural
organization: conclusion_first | stepwise | narrative | compare_contrast | problem_solution
exemplars:
  - text: "1–3 sentence snippet"
    note: "What this snippet demonstrates"
  - text: "1–3 sentence snippet"
    note: "What this snippet demonstrates"
---
```

### Prompt-and-Rerank
For rewrites: run multiple LLM samples, then use the local embedding model to rank them against the style card vector. Show users a "style update grade" based on vector comparison.

## Generative Learning Goal

When users with a downloaded knowledge pack add their own unsorted notes, the suggested connections and LLM fact-checks help them understand and integrate the knowledge — not just store it.
