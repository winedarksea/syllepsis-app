# Syllepsis Design Docs

Syllepsis is an open-source note-taking app for building "books" — large, unified knowledge spaces — rather than quick notes or to-do lists. Notes begin as rough captures and are gradually organized into a connected graph, then a tree, then a continuous narrative. LLMs accelerate every step but are fully optional.

The app stores everything as plain markdown files on the user's device, synced to their own cloud storage (Google Drive, GitHub). There is no hosted backend.

---

## Documents

| Doc | What's in it |
|---|---|
| [vision.md](vision.md) | What Syllepsis is, example books, user stories, guiding principles |
| [core-concepts.md](core-concepts.md) | Unsorted vs. sorted notes, graph→tree→book progression, sorting model, categories, knowledge packs |
| [object-types.md](object-types.md) | All note/object types, metadata schemas, file format, cleanup rules |
| [ui-views.md](ui-views.md) | All UI views (book, graph, search, constellation, stats dashboard, etc.), device UX |
| [theme-style.md](theme-style.md) | Practical Material Design 3 style guide for the Nordic/Icelandic visual theme |
| [spatial-worlds.md](spatial-worlds.md) | Worlds (geo + image-backed), `loc:` coordinates, overlays, mind palaces, map view |
| [llm-ai-features.md](llm-ai-features.md) | LLM integration, fact checking, local embeddings, style cards, generative learning |
| [search.md](search.md) | BM25, vector search, RRF, category filtering, text-fade focus mode |
| [sync-backup.md](sync-backup.md) | CRDT, git versioning model, Google Drive + GitHub, conflict resolution |
| [platform-infra.md](platform-infra.md) | Tech stack (Tauri + Rust + Lexical), PWA limitations, build targets, plugins & themes |
| [privacy-security.md](privacy-security.md) | Private categories, locked files, unlock delay, deletion delay |
| [open-questions.md](open-questions.md) | Outstanding design questions, notes to self |
