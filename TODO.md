# TODO
Test the merge, split, and fork note tools

Device storage vs app storage for the app (like Obsidian, able to choose with more permissions for the book)

Need to make a GitHub Actions flow that publishes to syllepsis.org (our domain) a landing page as well as the built installer binaries available for download (likely all hosted on Cloudflare Pages). May also need to build the wasm bundles for the built in plugins.

When the user is going across the world map, it shows the coordinates (useful). What might be nice is when a user clicks (perhaps click, drops a temporary pin, then has a 'copy' button), it copies those coordinates (perhaps world / x /y) such that they can paste them exactly into a note's location metadata.

The current "dots" for notes placed on worlds can be difficult to see when the world map/image is colorful.

Add basic OTel logging / telemetry option

Add Obsidian frontmatter metadata mapping in the text importer. Likely includes created/updated which are ISO 8601 strings, tags, possibly alias/status

Consider adding "aliases" and "status" to the metadata for the notes.
stylistic_elements are not used

Graph
Future extension: have an option to drag to select notes, then run the "merge notes" tool to combine those notes.

Badges on notes (shown in notebox). Badges should be able to vary by theme.
"Evergreen" badge: A note that the user searches for and opens constantly.
"Dusty" badge: A note that hasn't been opened in 2 years (maybe it needs archiving).
"Orphan" badge: A note with no links to it and no tags (hard to find).
"Cluster Hub" badge: A note that your vector model identifies as the absolute semantic center of a massive cluster of other notes.

Settings
Settings should be able to edit the prompts for each LLM tool (with a "restore to default" option too).

side by side note comparison

Improve the icons for the default themes

Stats view
Show stats that reward the user for making progress on adding and sorting notes. Maybe a new trending or pattern page. Like a fitness app, encouraging fitness. A "completeness" or build out statistic.
The first stats page, Overview, could be expanded. The grid of "one number tiles" is nice, and we could fill that out as a full screen of number tiles, with each tile quite simple.

Embeddings need to be switchable. We will have only one bundled version, but we may update that in a year or two. Also if users select another (heavier or lighter) embedding model, it needs to regenerate all embeddings.
Future extension: see if we can get "generate summary" working on images too. (Image to Text LLM)

It's not clear the setting `Unload model after idle (seconds)` is worth it. It might actually be wasting compute to load and reload it (the embedding model is actually fairly small in memory, only 200MB which is fine here). It should maybe only clean up on app exit?

Book view should maybe have a toggle for "show tree". Showing the tree would show for cateogries and notes any other side branches that don't fit the main flow (just showing title and summaries for these).
Book view needs a clear layout on screen size, and no overlap of the table of contents

Tree view and recommended view that sorts in a 'proposed' book order (via embedding or possibly LLM) and then users can drag or confirm the sort to speed up note sorting

Privacy
"leave off managed cloud sync" (OpenDAL managed) capability — deferred, see [docs/sync-backup.md](docs/sync-backup.md#deferred-exclude_from_cloud_sync) for why (both sync engines need to become metadata-aware plus a cloud cleanup pass for already-synced notes).

Review what happens if a user, using git and Loro, was to rollback to an earlier commit version of their notes.

It's prompting for use of the keychain all the time (presumably whenever a cloud sync needs to happen), and it's always three requests back to back. Can't we store the token in some way that we don't need to ask permissions to use it every time? We want it to be secure, but if users have to do this more than once per session (ideally never, after first setup of cloud sync) they will not use the app.

Categories and commentary don't get Loro management

Test knowledge pack import/export, and import of a new version of an existing knowledge pack

Consider aligning the types with ts-rs or tauri-specta. tauri-specta requires annotating all ~149 commands and only helps the Tauri boundary (your REST/MCP servers in rest.rs and mcp.rs wouldn't benefit). ts-rs is a smaller change and helps all three boundaries, but leaves the command-wrapper layer hand-typed.

Kanban style graph
status, importance, archived, category
Option to filter down the cateogories shown and only show some cateogories. Option to filter down by importance. Option to not show the "no status" notes and section.
Option for what to color by (could be classification (or type) of note, could be by category (of those present), could be importance)
Kanban state should persist between sessions
Users should be able to drag notes to a new section to change the status (only can change to open/active/done this way). They can also click on notes to go to the note editor to edit it. If users drag a note to "active" section, that should set the "started" date as today (if not already set). If users drag to "Done" that should set the completed date for today (if not already set). In addition to dragging, perhaps in this view we should have a small icon on the note that, when clicked, opens a dropdown to change the status (showing all seven options).
Cancelled is the right most item and shown faintly
Resize sections a bit by number of items in each section (but not too extreme).
The seven states would be grouped together on the Kanban board in three sections, with icons to show the subtype where relevant
To-Do: No Status, Open, Deferred, Needs Clarification
In-Progress: Active
Done: Cancelled, Done (cancelled notes would go below the done notes vertically, and be grayed out or otherwise styled in a reduced manner)

Link note to drawing doesn't work

Show more of the editing section in editor view. Be able to collapse it down

Improve layout of Notebox topbar

Improve note importer (with house example)
