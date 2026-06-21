# Open Questions

Design questions that haven't been fully resolved.

## Active Questions

- **Multiple style copies of a note?** Should a note be able to store variants in different styles simultaneously, or is one canonical version + commentary sufficient?
- **Fact checking with metadata and references**: how do references get explicitly linked to fact-check results? What's the UX for showing a fact check alongside its source references?
- **Preference placeholders**: for knowledge packs that contain notes with user-specific preferences (e.g. "your name here"), how do we make it obvious which items need to be replaced by the user?
- **CRDT-tracked drawings**: should simple *app-authored* SVG drawings eventually be CRDT-tracked (since the app controls their structure), while imported SVGs stay file-synced? Where's the line, and is it worth the complexity? See [sync-backup.md](sync-backup.md#drawings-and-svg).
- **Image-world coordinates**: normalized `0..1` is the plan (survives re-export at new resolution) — confirm this holds for very large floorplans and multi-floor grouping, and decide how floors of one building are grouped as related worlds.

## Example Knowledge Pack Ideas (Future)

- **Christian knowledge pack**: history of Christianity's evolution; aimed at the kind of Christian that is thoughtful and historically aware. Should be fair and cover both the positive tradition and the historical patterns of violence. Separate from the American evangelical tradition.
- **Atheist knowledge pack**: counterpart to the above.

These would be examples of how ideologically or culturally loaded topics can be handled in a balanced, well-referenced way inside the knowledge pack framework.
