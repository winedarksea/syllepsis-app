# TODO
The simple string search in the editor works well in "Read" mode, seems a little quirky in "Source" mode (it jumps to the section of the next fine but sometimes that is one line below the visible part on the screen), but doesn't seem to work at all in "Edit" mode.

We also need to add a merge note tool. A simple merge just adds the two notes together, including metadata.
There should also be a simple split note tool, that just slices the note in two at a given point (mostly inheriting the parent's metadata for both).

Device storage vs app storage for the app (like Obsidian, able to choose with more permissions for the book)

Need to make a GitHub Actions flow that publishes to syllepsis.org (our domain) a landing page as well as the built installer binaries available for download (likely all hosted on Cloudflare Pages). May also need to build the wasm bundles for the built in plugins.

A collection of related functions:
Confirmation delays on delete: No 24-hour confirmation requirement before deletion
Fact-checking as a first-class workflow: Infrastructure exists but no dedicated fact-check command with the assertion "strong evidence | questionable | many issues | full failure" enum

We have no support for making drawings yet. 

When the user is going across the world map, it shows the coordinates (useful). What might be nice is when a user clicks (perhaps click, drops a temporary pin, then has a 'copy' button), it copies those coordinates (perhaps world / x /y) such that they can paste them exactly into a note's location metadata.
try to upload a fancier SVG /Users/colincatlin/Downloads/preview-mapome.svg, we get an error: `parse error in SVG: XML with DTD detected`. It would be nice if we could work past this (perhaps a simple strip of XML data while still keeping an SVG is possible?).
The current "dots" for notes placed on worlds can be difficult to see when the world map/image is colorful.

Existing "knowledge" pack exports do not preserve prior. They should. We might want to include knowledge pack links as metadata (or perhaps as a special type of category)

Add basic OTel logging / telemetry option

Add Obsidian frontmatter metadata mapping in the text importer. Likely includes created/updated which are ISO 8601 strings, tags, possibly alias/status

Title vs summary, needing both is a litte confusing

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

Stats view
Show stats that reward the user for making progress on adding and sorting notes. Maybe a new trending or pattern page. Like a fitness app, encouraging fitness. A "completeness" or build out statistic.
The first stats page, Overview, could be expanded. The grid of "one number tiles" is nice, and we could fill that out as a full screen of number tiles, with each tile quite simple.

Embeddings need to be switchable. We will have only one bundled version, but we may update that in a year or two. Also if users select another (heavier or lighter) embedding model, it needs to regenerate all embeddings.
See if we can get "generate summary" working on images too. And make sure images have embeddings (either directly from image if supported by model, or via the text summary)
Review how images are handled with metadata.
Make sure deleted images clean up all items as appropriate, sidecar might be missed. Also items uploaded, but a user clicks "cancel" before they are finished with import seem to not be cleaned up and remain.

It's not clear the setting `Unload model after idle (seconds)` is worth it. It might actually be wasting compute to load and reload it (the embedding model is actually fairly small in memory, only 200MB which is fine here). It should maybe only clean up on app exit?

Book view should maybe have a toggle for "show tree". Showing the tree would show for cateogries and notes any other side branches that don't fit the main flow (just showing title and summaries for these).
Book view needs a clear layout on screen size, and no overlap of the table of contents

Tree view and recommended view that sorts in a 'proposed' book order (via embedding or possibly LLM) and then users can drag or confirm the sort to speed up note sorting

Privacy might need another setting, whether it just means hide from most screens. We should probably split it into: gitignore, exclude from search, and hidden (with hidden defaulting to including all three but really meaning not shown in main UI exports). Perhaps work considering if we can add also a "leave off managed cloud sync" (OpenDAL managed) option as well.

The special types of notes that "require" a delay rely on commentary to make this happen.
Unlock delay enforcement: 24-hour gate on unlocking files
Fact-check gate for locking: require LLM verification before unlocking
Fact-checking as a first-class workflow: Infrastructure exists but no dedicated fact-check command with the assertion "strong evidence | questionable | many issues | full failure" enum

Note types need review. We can probably get rid of separate note creatioins for: 'Q & A', Reference, Quote (these can be done by selector in a standard note). Code can be auto-detected by using markdown code block as the only element.

It's prompting for use of the keychain all the time (presumably whenever a cloud sync needs to happen), and it's always three requests back to back. Can't we store the token in some way that we don't need to ask permissions to use it every time? We want it to be secure, but if users have to do this more than once per session (ideally never, after first setup of cloud sync) they will not use the app.

Categories and commentary don't get Loro management

A likely sync optimization is remote revision tracking: use provider metadata like Drive file IDs/version/modified time/hash where available, or maintain a small remote manifest, so sync can avoid reading every remote file just to prove it is unchanged.
