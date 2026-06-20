# Vision

## What is Syllepsis?

Syllepsis is an open-source note-taking app designed for "books" — large, unified collections of ideas — rather than quick notes or to-do lists. It sits somewhere between book-writing manuscript software and a classic note-taking app.

The central workflow is progressive organization: notes start rough and unstructured, then are gradually categorized, linked, refined, and finally woven into a single coherent narrative. LLMs accelerate this process but are fully optional; the app works without them.

## Example Books

- **Life philosophy / self-help**: mixing science-backed evidence, personal principles, opinions, and practical tips.
- **Residential home design document**: desired outcomes, building code snippets (with references), evidence-based energy efficiency principles, and style-based design principles (e.g. Japanese aesthetics) that may or may not have scientific backing.

These examples illustrate the core tension Syllepsis embraces: structured enough to write from, flexible enough to hold messy, evolving ideas.

## User Stories

- Build a better knowledge structure for large projects or big ideas, fact-check claims, and use the notes as context in LLM conversations.
- Write a book (or detailed outline) and export to a final word processor for completion.
- Manage a todo/kanban board inside the same knowledge store as the research behind those tasks.
- Maintain separate "books" (life philosophy, research projects, todo board) that LLMs can reference together while working.

## Guiding Principles

- **LLMs are widely integrated but optional.** Most functionality works without any API key.
- **Generative learning.** Suggested connections and fact-checks help users understand knowledge, especially when starting from a downloaded knowledge pack.
- **Long-term stability.** Users should be able to upgrade app versions without losing access to any notes. Breaking changes are acceptable early; stability is the long-term goal.
- **Open, portable data.** Notes are plain markdown files. Export, import, and serve to other apps so the library stays simple but extensible.
- **Modular codebase.** Small files, fewer lines. Easy to adjust during the initial POC phase.
- **Plugin-friendly.** Design for future plugins that are sandboxed or WASM-based for security.
