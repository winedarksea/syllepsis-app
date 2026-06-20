# LLM & AI Features

LLMs are widely integrated but **fully optional**. All core functionality works without an API key.

## Cloud LLM Uses

- **Fact checking**: throw out a hypothesis, then have an LLM (prompted for groundedness) assess what is scientifically known. Fact check results are stored as [Commentary](object-types.md#commentary) nodes with a status enum (`strong_evidence`, `some_questionable_points`, `many_questionable_points`, `full_failure`).
- **Devil's advocate**: a prompt type that specifically seeks potential flaws in an argument.
- **Summarization / description generation**: generate a summary from full text or expand a summary into full paragraphs (guided by a style card and metadata).
- **Writing quality / grammar check**: stored as commentary with a status like `needs_rewrite` or `minor_issues`.
- **Category suggestion**: combine clustering results with an LLM call to suggest new or refactored categories.
- **Notes as LLM context**: surface selected notes to LLMs so users can ask questions grounded in their own knowledge base.

LLM response types are an extensible family — new prompt types can be added without changing the core commentary architecture.

### Proposal Flow
When an LLM rewrites a non-empty section, a clear accept/reject proposal is shown before any overwrite. Users can enable auto-accept upfront. Rejected rewrites are discarded; accepted rewrites replace the original (with an option to archive the old version as a commentary node).

## Local Embeddings

Embeddings are computed locally using a model like [BAAI/bge-m3](https://huggingface.co/BAAI/bge-m3) (~8000 token context). Vectors are stored in [LanceDB](https://github.com/lancedb/lancedb) (or fastembed-rs + sqlite-vec for WASM compatibility).

### Multiple Vectors Per Note
- One vector for the **summary**
- One or more vectors for the **main body**
- Notes longer than ~512 tokens are chunked; each chunk gets its own vector

### Category Vectors
A category's vector is computed as an average of its member notes' vectors. This enables category-to-category similarity (duplicate/near-duplicate category detection) and the category upweighting used in the [related carousel](ui-views.md#related-carousel).

### Uses of Embeddings

| Feature | Description |
|---|---|
| **Vector search** | Semantic search across all notes |
| **Clustering** | Suggest new or reorganized categories |
| **Coherence analysis** | Detect consistency or narrative flow issues |
| **Duplication detection** | Surface most-similar notes and most-similar categories |
| **Blind spot detection** | Sort by *reverse* similarity to neighbors — lowest scores suggest disconnected narrative or missing content |
| **Style comparison** | Compare a note's style vector against style card vectors |

## Local LLMs

Long-term goal: integrate native platform LLMs (Windows AI API, Apple Intelligence, LiteRT-LM) for simpler tasks where they are sufficient:
- Proofreading
- Summarization
- OCR (WASM / WebNN are an alternative path)

Users can configure **per-task routing**: choose which provider handles summarization, which handles fact-checking, etc. Options per task: native local LLM, various cloud LLM providers (cheaper models for high-volume tasks).

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
