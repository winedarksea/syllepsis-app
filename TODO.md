# TODO
Device storage vs app storage for the app (like Obsidian, able to choose with more permissions for the book)

Need to make a GitHub Actions flow that publishes to syllepsis.org (our domain) a landing page as well as the built installer binaries available for download (likely all hosted on Cloudflare Pages). May also need to build the wasm bundles for the built in plugins.

A collection of related functions:
Unlock delay enforcement: 24-hour gate on unlocking files not enforced
Fact-check gate for locking: Can't require LLM verification before unlocking
Confirmation delays on delete: No 24-hour confirmation requirement before deletion
Fact-checking as a first-class workflow: Infrastructure exists but no dedicated fact-check command with the assertion "strong evidence | questionable | many issues | full failure" enum


No support for images yet.
Worlds page needs support for creating new worlds.
Earth map should maybe get a starter basemap using an Equal Earth or orthographic projection or similar SVG based (rather than basemap tile based) projection. The goal is being as detailed as possible while still being very lightweight.
If reasonable, our plan was to use something like a 1:10 million scale map, with Rust handling the heavier math with the `georust` library of crates, and if necessary d3-geo or react-simple-maps for frontend
Use case examples: a user might have a map of every place they've visited around the world, so country level markers. A user might plan a roadtrip across the US, and then we need a nice zoom in of the continent level North America to see that. A high quality continent level view (ie all of Western Europe in the view field) is the highest precision we need to support, and smaller just gets too heavy.

Existing pack notes do not preserve prior. They should.

Add basic OTel logging / telemetry option

stylistic_elements are not usable, and style cards need some work, and some defaults

Add Obsidian frontmatter conversion to text importer.

Title vs summary, needing both is a litte confusing

"Search" seems to rank empty notes rather too highly
Search should have context filters for freshness (when last updated), length, note type, starred, and categories

Categories
Give the option to map categories to a specific location (the same world/lat/long pattern as elsewhere)
Be able to click on a category in bookview and go to its settings (just like when clicking on categories on the sidebar)
categories should have both a hashtag (no whitespace) name and a full nume. We should see both in the category metadata settigns
for some reason, when we click on the categories on the sidebar, it shows the notes linked to a category, but it is missing some. Perhaps it is ignoring "sorted" notes here? It should show all notes with that category tagged.
The categorie's average vectored (stored) should also be shown here.

Graph
Timeline and nodes on the graphs should all click-through to the given note.
We should have the toggle for "Prior relationships" be a toggle option for all graphs, as being able to turn if off on clusters might be useful as well.
Timeline aggregation to "Month" showed nothing. Aggregation to "day" also was weird (it had a line but none of the "lollipops" of the notes visible). The other aggregations worked fine.
Future extension: have an option to drag to select notes, then run the "merge notes" tool to combine those notes.

Notebox:
add a sort order for "starred"
in addition to "unsorted" and "all notes" also have an option for "uncategorized" (ie no category tagged) and to include archived (archiving is not fully implented yet).
default sort order should be for most recently updated.

Badges on notes (shown in notebox). Badges should be able to vary by theme.
"Evergreen" badge: A note that the user searches for and opens constantly.
"Dusty" badge: A note that hasn't been opened in 2 years (maybe it needs archiving).
"Orphan" badge: A note with no links to it and no tags (hard to find).
"Cluster Hub" badge: A note that your vector model identifies as the absolute semantic center of a massive cluster of other notes.

Settings
Settings should be able to edit the prompts for each LLM tool (with a "restore to default" option too).

side by side note comparison



We need three modes for the note editing screen: reading mode, editng mode, and raw (or source) mode (rather than just "raw" and "rich text").
The reading mode is the same as the default view for book view. It shows nicely rendered markdown, no toolbar of text editing tools (the bold, italics, etc options). Spoiler (clozure) tags are render as hidden. URLs are clickable and should open in browser if clicked. Compare that to editor mode where URLs when clicked give an option to edit the link, and to raw mode, where the raw URL format of markdown is shown. All are the same underlying markdown, they are just different ways of seeing and using it.
Currently some of the text editing tools act a little weirdly. For example, if the "bullet" tool is clicked, it makes the whole note a single bullet point, ignoring the new lines that are present. Simple things like bold and italics seem to work fine.
In these views we should be able to do a simple "command + F" find to do exact regex string matching and jump to the first result (and arrow click to the next result).
Currently "crtl+z" the standard 'undo' command works fine in these editors. We should probably add a unittest to make sure that continues to work in the future as it is very useful.
In addition to "back", there should be arrows to switch to the previous and next notes (when sorted).

Implement autocomplete for categories when a user starts with a hashtag (reuse also for linking with the @) when in editor mode. We would also likely appreciate auto suggestion help for dates (such as due:) and for locations loc:, and for the blocked/waiting references.
If practical and efficient, it might be neat to have the category autocomplete show (and sort by) the similarity between the category's average vector and the note body's vector.

Users should be able to click on the category in "details and metadata" and have that take them to the category summary (metadata and other linked notes for that category)

Show character count or token count in editor view to encourage shorter notes. Maybe it a more 'warning' color above 2k tokens worth.

Relates notes are pretty good the way they are, although perhaps we should have an option to hide them to free up more screen space on mobile.

Expand to show option for the embedding vectors of that note and the note summary in the Details & Metadata section

The delete button in the note editor needs a quick "confirm delete" dialogue to prevent accidental clicks deleting the note.

We likely need to revamp the "Tools" dropdown, so a tool like 'generate summary' can be chosen to be local llm or cloud for that run, and set options like to use mnemonic or acrostic summaries. It might make more sense as an opening dialogue of some kind. LLM tools should be adding to the same queue as the embedding models, and so are an async operation that doesn't block the full app (it could maybe block just the notes and summaries until the queued job is finished). Users may navigate away and so we may need to save the response (we have the _commentary folder for this, unused so far it seems) so they can apply it when they go back (and perhaps get a little pop up "job done" with a link to head back when it is done).
"Rewrite body" is another tool with plenty of options. It should be able to use a style card, or a "simplify" prompt that proposes an updated note with obvious redundant parts removed or simplified.
We also need to add a merge note tool. A simple merge just adds the two notes together, including metadata.
There should also be a simple split note tool, that just slices the note in two at a given point (mostly inheriting the parent's metadata for both).
