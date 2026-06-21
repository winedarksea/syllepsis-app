# This file contains the raw notes (broken down here in /docs into separate files)
The goal of this is to design a type of open source note taking app with the name Syllepsis.
This creates and refines "books", larger collections of unified ideas, not "quick notes" and short to-do lists like most note apps target. Really this is meant to be somewhere between a book writing manuscript software and a more classic note taking app.
Here are two examples of books that would be constructed with this:
The first is a book of life philosophy/self help book (generally science based with evidence but mixing in opinions, principles, and tips of various kinds). The second is a long design document for a residential home, which includes desired outcomes, opinions, snippets of building code (with references), science based evidence of good design principles (say for energy efficiency), design principles not backed by evidence necessarily (for example design principles for Japanese style, some of which may be evidence based value, and some of which may just be style).
Core ideas:
A primary goal is to be able to handle "unsorted" notes. These are a type of "quick note" aimed at later merging, organizing, and refining into the overall structure of the book.
The goal of a "book view" is for the notes to be viewed as a single continuous document but actually with each note separate behind the scenes. This only applies to sorted notes. Inherently a sorted note is one that has a clear place in book view. An unsorted note does not. There will likely need to be a hybrid book-tree-graph view (for large screen devices), so on the left side of the screen are the sorted notes, and branching off from them are the linked (ie share the same categories) unsorted notes. This view is going to need some careful design, as there may be many possible linked notes to be able to "scroll through" on the side, separately from scrolling through the book view. For even more complication, unsorted notes may contain sorted fragments, for example 5 items sorted together, but not sorted into the main narrative yet (these are 'branches').
Generally notes start as unsorted and uncategorized. Ideally right away they are categorized, linking them into a graph view. The graph is gradually refined into a tree, and then finally into a single branch narrative 'book' view. Of course, users may choose to stay in graph or tree levels of organization, and generally the goal is to improve organization, however it is useful for that knowledge and user.
One of the features we are most excited about is being able to use LLMs to run fact checks. The idea being we can throw out crazy ideas, then have have an LLM api call prompted to ground the ideas on what is scientifically known.
These notes should be surfaced as context to LLMs, so users can ask questions using their notes to help guide answers.

One book that is a Todo Kanban board. One that is a setup of life philosophies and goals, another book for any major research projects. Each is separate, but LLMs can reference across while working.

LLMs are widely integrated but still optional part of the design, the goal is for this to fully work with most functionality without them.
Include analytics. Track how often a note is pulled by LLMs, track how often opened by users, etc, to rank its usefulness
Being able to import, export, and serve to other apps such that we can keep this library relatively simple but yet allow a lot to be done with it.
Being able to view and export this in a book like format. Generally this is intended for "large" idea spaces on a central theme or large project, not loose unrelated notes. 
Design for being able to add "plugins" easily in the future, and these plugins be secure (such as sandboxxed or wasm).
Support for multiple themes, including custom user themes for the UI, starting themes are a light mode and dark mode.
It would be nice to allow users to be able to load "knowledge packs" elegantly into an existing book, these being a preorganized selection of notes on a topic.
Perhaps long term there would be a hosted marketplace of plugins, themes, and knowledge packs
It should be accessible from laptop, phone, and tablet (Mac, Linux, Windows, Android, iOS). It might be worth considering a web app downloaded to phone that works offline rather than through an app store (perhaps a progressive web app).
Native spell check, auto-save, on all platforms would be nice to have.
This needs high quality backups and cross-device sharing. Users will connect their own cloud sync services (for starters, Google Drive and GitHub) as this app will not provide its own cloud hosting. Likely something like https://github.com/yjs/yjs or https://github.com/automerge/autosurgeon to handle merge across devices. Also Loro https://github.com/loro-dev/loro
Crdt doesn't track images. Likely uses UUiD for sidecar so it handles file moves. Actively manage cloud conflict files with merge and delete (by UUID presumably). Implement mitigations to prevent infinite write loops.
Git is likely a dependency, but used a little differently. Commits become a bit more like official versions or releases, whereas the basic saves and cloud syncs (ie drive) happen in near real time.
It's generally expected to be one user to one knowledge store, so it doesn't need to be a high volume or costly design. However being able to share with others is a nice to have extra.
For example, we might expect this to be synced to both GitHub and Google Drive at the same time. The Google Drive contains the full backup, and anyone with full permissions to the drive can access (but aimed at just one or two people, perhaps some with read only access). Meanwhile some notes are marked as private, and the GitHub publishing (possibly via gitignore) excludes these notes, with the github publish being the full public version.
For initial development, breaking changes are expected frequently, but long term the goal should be that users can upgrade the app version without losing access to any of their notes.
Need good tests. Users need to trust they can not lose their insights easily.
Search
	The search function needs to be very good. Both by exact match string search, sparse retreival (bm25) and vector match, perhaps alongside Reciprocal Rank Fusion (RRF).
	Search should include an option to filter down results by category (rather like an e-commerce search).
	Finally, when a note is selected, it should allow for a user to open that in a graph view or the book view, and then modify that, or add a new note in as its neighbor. It is common for me to search for a phrase I remember near where I want to put a new note, so this would support that workflow.
	Search might have an option (in book view or in long documents) to make fainter (ie more gray) text which is less relevant to an input search term, to help users focus in on the most relevant content.
Cleanup:
	Be able to archive notes (which means not showing up in RAG or default views but can toggle on)
	Have notes that are set to self delete on creation after so many days (180 days default, for example), 'vanishing' notes
	Pictures don't have an "archive" option, just delete (anything that takes up lots of space, don't archive but delete). Actually delete is "mark for deletion" then delete 30 days (configurable) later (runs cleanup on startup or user action probably, no need to be exactly 30 days to the second)

Local math, the goal is to have advanced features powered by local embedding models (not reliant on cloud LLMs).
Embedding, something like 8000 tokens https://huggingface.co/BAAI/bge-m3
https://github.com/lancedb/lancedb
Uses of the embeddings:
	Clustering to suggest new categories (can combine with a full LLM call to suggest new or refactored categories based on clustering, perhaps)
	Coherence of narrative analysis, consistency
	Duplication, show most similar notes, and also most similar categories (categories get assigned some sort of average of their components)
	Blind spot detection: sort the results in reverse order to find the sections that have the lowest similarity scores to their neighbors. This suggests unconnected narrative.
	Vector based search
	We will have multiple vectors for a document. One for the summary, and one or more for the main body. If a document is longer than 512 tokens, it likely needs to be chunked, and we need to track vectors for each chunk.
Local LLMs:
In the long term, support integration with native LLMs (Windows AI API, Apple Intelligence, LiteRT-LM, etc) although these might need to standardize a bit more first. There are separate from the LLM APIs, they are local LLMs doing simpler tasks: proofreading, summarization, OCR (WASM and WebNN might be another way to handle this). Behind the scenes, users should be able to select where they want each of the various categories to go to, for example summarization would have the option of native local summarization, various cloud LLMs (likely using a cheaper one here), etc.
It would also be good if this could be integrated as "native" context to a user's primary LLMs, for example Google Gemini, ChatGPT, etc. For Google, just uploading it to Google drive (as backup) may double as integrating it into Gemini context.

Goal: we want to encourage generative learning, especially for users who have already started with a large downloaded knowledge pack. When they add their own unsorted notes, the suggested connections and LLM fact checks should help them understand the knowledge

Object Types:
Each note is an object. Most data would be stored as a type of string object. String objects are broken down into various sub types of string objects. These string objects are stored as markdown text files.
YAML frontmatter stores the key metadata but it is hidden from the standard ui view (metadata there being handled by a UI metadata input area). Perhaps fenced blocks instead of yaml would be fine.
There are several note types that are special types. Tables would store as a csv (again with YAML frontmatter). Pictures are a special item types, captions and other metadata would be stored in the XMP metadata, using the same markdown format for text metadata like captions, which this app would read and write to. The goal is to support PNG, JPEG, GIF, SVG, and WebP.
This means that pictures, tables, and so on are not put strictly into text notes at all, but they can be sorted just before/after text notes, and text notes can link to them. The UI viewer then has the option to include them (pictures, etc) embedded in text at the link point, as a click and expand item, or as a link to follow through to a dedicated view.
Tables have special subtypes, decision matrices and pro/con tables.
Code blocks as a special text type, with Mermaid a special type of code block that can render the diagram, including Venn diagrams, a type of mermaid diagram.
Quotes (ie famous people's sayings) as a special type, includes links
A "drawing" type (stored as SVG) allowing drawings with built in render to image where needed. Imported SVGs are treated as drawings (no separate type); the future in-app drawing tool emits SVG too, so both share the same overlay/anchor tooling and are the preferred backdrop for image-backed worlds. SVG is text so it diffs in git, but drawing geometry is file-synced (UUID sidecar), NOT CRDT-tracked by default — only the small overlay anchors (note↔coordinate links) are CRDT-tracked. Whether simple app-authored drawings could later be CRDT-tracked is an open question.
Note IDs (auto generated) should be human readable to some extent. Format is `{type}-{slug}-{ulid}` with no colons (so the same string is filename-safe everywhere), e.g. `quote-montaigne-on-friendship-01jh5k3q2x9y8w7v6t5s4r3q2p`. The ulid is the canonical immutable identity (decentralized, collision-proof); the type+slug is cosmetic. Canonical id lives in frontmatter, not the path. See [object-types.md](object-types.md#note-ids) for the full scheme.

Examples apps worth referencing for comparison: LogSeq, Obsidian, Tana, Trillium, Plottr, Mem

Smaller Implementation Notes:
Allow use of double percents %% Your comment here %% to add comments that don't show in rendered view of text markdown.
Allow ||spoiler|| syntax sugar for click-to-reveal spoiler text. The same spans double as cloze deletions for study/learning (hidden in a study mode, recalled then revealed). Optional hint and group: ||hidden|hint|| and ||c1::hidden|| to group deletions that reveal together.
Book level metadata: preferred language, name, location (for example, for the house, the city of construction for LLMs to be able to lookup relevant construction code). Book Level metadata also stored as a markdown file.
Likely each book is a folder in storage. We could maybe have subfolders here, if their notes are sorted in book view and then store notes if sorted into their main category as a subfolder.
Add icons to each book so it is prettier. This is called the "cover" and in the long term might allow a drawing interface to create (for now just an image load, supporting SVG, JPG, PNG). Also allow icons for categories.
UI Views
	Have a broken link view to allow users to see where links may need to be updated. There would likely be a dedicated tab of 'diagnostic' and 'repair' UI views together. For example "orphaned notes" section helping to find and fix. "Blind spot detection" (from embedding model) view for gaps in notes suggested by the vector space.
	An important view is the unsorted, uncategorized note view. This view is focused on being able to categorize, dedupe, and refine these "quick notes" better into the structure.
	There should be a category view that starts by showing categories, diving into them, focused on categories more than notes, but allowing to dive into notes from there.
	LLMs will need a view to manage prompts and to manage api tokens/provider connections
	Have a search view that, once the search is entered, puts the search at the center of a web of related content. Include the option to "start a chat (with an LLM)" from context selected from this view, or to click into notes and read or edit.
	Related carousel view, where notes are surrounded by other notes (similarity vector) as well as category upweighting of similarity
	Constellation star chart with solar system zoom in view
When writing the app, aim for small files. Modular. Aiming for fewer lines of code so the initial POC is easier to adjust as needed.
The long term goal is WYSIWYG markdown creation, but initial POC does not require that.
Links should be able to point to sections inside of a text object, if it uses markdown headers for sections. Generally the goal is for users to break down large text into smaller individual objects, but some cases this may not make sense (for example if the user imports an essay from a blog, and wants to keep that content all together as written).
Add notes, two ways:
	A "plus" icon that shows up on hover near the link between two notes that allows to insert one sorted into the spot between those two
	A "new note" buton that adds an unsorted note (can still have categories, just not a sorted position in book). If another note is selected when new note is clicked, the new note defaults to the same categories as that note
Have a stats dashboard UI view to show vector alignment between views, also update times, and other analytics captured. The primary goal of analytics is to understand how useful a note is, and if the user is actively referencing it (or actively being used for them via LLM retrieval, etc).
Have a read only server view option (can search and view, but cannot edit), that can be published as a website separately (ie edit view is private port, local view is shared on internet). This would likely be a PWA too.
Editing views need to adapt to device size.
Android app should allow touch and drag to reorganize inside a category. A common workflow might be typing on a laptop where it is easier to type, then reviewing and organizing on a tablet where it is easier to drag items around.
Prompts new users with no books to download the example books

Styles: similar to categories, styles are how the text is written (likely description and vector(s)).
Likely this could come as a downloadable knowledge package, a way for users to compare their style to know styles (presumably vector comparison, or simliar), but also to create their own styles from an input text. When generating a paragraph from a summary using an LLM, it could perhaps be given this style card linked to a note as the preferred style, or to rewrite with a given style a passage.
Consider Prompt-and-Rerank for rewrites which is running multiple samples, then using the vector reference of the local embedding model to compare. Also show users a style update grade based off the vectors.
Style cards include optionally urls to openly accessible sources of that text (for example, shakespeare sonnets for a shakespeare sonnet style).
For using a style vector, should also link clearly the model used for that vector (possibly storing a key value pair of vectors for multiple embedding models).
Styles cards should be versioned, to support future attribute updates.
Creation of a style looks something like this: provide a corpus, create embedding vectors, discover examples (exemplars) using the embeddings, for example the top 5 1-3 sentences from the corpus that are most emblematic of the style and store those as examples in the card. Then pass to an LLM exemplar pieces of the corpus (probably multiple paragraphs here) and ask it to do a first pass on the style enum and short description, then have a human review and finalize the style enum.
Example potential style_card:
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

Text Objects and General Metadata:
One key idea is that text objects have two "views" of the same concept: a summary and a full text description (ie a paragraph view, although not necessarily full paragraphs). This would enable users to view stories or categories only seeing the summary, then click on cards to see the full description, rather like a flashcard.
LLMs could be used to create one from the other, either summarize the full description or write a description from a summary (using style card and metadata as guides).
The generic "create summary" button should also offer format options to prompt for a mnemonic summary: an acronym (memorable word/initialism from the key points) or an acrostic (lines whose first letters spell a word). These pair naturally with cloze-deletion study.
When an LLM rewrites a section, there should be a clear proposal with accept/reject for the updates if it is overwriting a non-empty section (users have option to auto-accept up front).
Summaries wouldn't strictly have a character limit, but there would be a warning display if, for example, the summary is longer than 250 characters or longer than 10% (configurable) of the full text description (whichever is larger). And generally a metric showing summary to full description size ratio, along with a vector similarity score, to help users never get them too far out of alignment.
There are several specialized text types: quotes, references, QA
Quotes: text plus reference. Meant to cleanly show something was written by another. 
QA: question and answer. Really just renames the standard "summary" and "detailed" parts of a card into question and answer and then both are shown, rather than one or the other. If the answer is just a single link, then the question is a special type that really just serves as a tag pointer to a section.
Future text objects: code (isolated in WASM) and dataview style query cells (https://blacksmithgu.github.io/obsidian-dataview/)
Worksheet
References:
	Author, Year. Title. URL (shown on hover). Tagged with @. Really the main difference between using a URL link and an @ is that the @ would always have the example same shown text, while URL could link from any specified text.
	Year refers to the published year, we don't track accessed year separately.
	References don't have summaries and generally are expected to just be a largely fixed collection of metadata
Date Metadata:
	Multiple datetimes tracked in metadata for each note. Creation date, last update date. Then users can optionally add a scheduled/target date and a completion date
	Tag a date as a reminder
	Have dates be "+N days" from another card, so they are all relative dates. In the future, potentially have a timeline view. Timeline is a special UI view, not a data type.
	Dates should have import/export option live with a calendar, in a future version
Location metadata: (see docs/spatial-worlds.md for the full model)
	Text, pictures, and all objects can link a location
	Location can just be a string of text. Separately there is a csv lookup table mapping text to coordinates for use in a map/overlay view. Users can click through the string to see/enter the coordinate
	Lookup table also references what "world" this uses. Most expect to use Earth, but the idea is this should be designed to in the future support fantasy world maps, other planets, OR image-backed worlds (a world that is just an image, e.g. the floorplan of the first floor of a house)
	lat/long should be a special inline type fit into markdown: syntax sugar like loc:47.6062,-122.3321 (Earth) or loc:firstfloor/0.42,0.31 (image world) to drop a coordinate mid-note or in a table cell; typing loc: opens a picker like due: opens a calendar
	Whole notes can also be tagged to a location via an optional frontmatter location field
	Map view that loads map tiles and shows geo-tagged notes is a FUTURE extension (needs tile infra). Image-backed worlds + overlays (floorplans, mind palaces) come first and need no tiles
	Mind palaces = tagging notes into locations on an image (method of loci) to aid memory. Not a new data type, just a book/pack whose primary lens is an image-backed world + overlay. SVG preferred backdrop
	Overlays: pins (points) and regions link notes/categories onto an image world. SVG named ids double as clickable regions; raster needs an explicit zoom/pan transform so pins stay anchored as the user zooms
	#category can carry an optional location/region (e.g. #kitchen as a clickable area on the floorplan that runs the category filter)
Authorship:
	Authorship tracking should be included in a very lightweight version for when multiple users present. Lightweight means not necessarily line by line, perhaps note by note (metadata tracks creator, then array of editors).
	Should enable AI versus human input tracking with this.
	Authorship would ideally trace to the identity provider (ie GitHub, Google account) of cloud sync, rather than managing locally. Would allow an alias for a friendlier name if identity provider username is opaque.
	Likely tracked would be "created by", "edited by" and "ownership". Created and edited are tracked and managed by the system. Ownership would be the author link that can be changed in the UI.
Forking
	Notes would support being forked (ie duplicated) from an existing note.
	Forked notes would include the forked parent id, and timestamp of the fork. Ownership would update to the author who made the fork.
Footnotes: users would be able to enter a third text body, in addition to the main text and the summary. This is a collection of text that is normally hidden, but available if a user wants to store more. Possibly the "commentary" object type below could replace this concept.
Include other kanban/scrum type metadata, such as assignee and magnitude, although mostly unused for now. The goal is to be able to use this as a todo list or kanban board, albeit as a lower priority and secondary functionality
Text Object Metadata Example (not finalized):
{
"statement_type": "hypothesis | factual_claim | rule_or_requirement | principle | preference | procedure | context | analysis_or_interpretation | narrative | idea", (idea is default)
"basis": "science_and_data | regulation_or_standard | logic_and_reasoning | tradition_and_culture | established_lore_or_fiction |  lived_experience | personal_preference | none", (none is default)
"checkability": "objectively_checkable | partly_judgment_based | subjective_or_personal | none",
"stability": "settled | evolving | tentative",
"priority": "standard| important | core",
"starred": "true | false"
"stylistic_elements": ["anecdote", "metaphor"]
}
Should include something like markdown_version:gnosis_app_001. Should include the app name in this version so users of the markdown outside the app can figure out the style's origin

Todo list as a special text type. It only contains checklist items (that's all the UI will show of the markdown), includes syntax sugar (see below) and an auto-archiving feature where items marked done or cancelled are moved after a configurable number of days to a todo archive file with a completed:date added. UI for todo has a simple action to drag them up or down in order on the list
Perhaps some more syntax sugar like this for todo status (we would probably support this in all text notes, but it is aimed at todos in particular):
Checklist enum:
- [ ] open (not started)
- [/] active
- [?] needs_clarification
- [>] deferred
- [-] Cancelled
- [x] Done
For dates:
due:deadline, ie due:2026-01-01 (when a user types due: it should open a datetime calendar widget to select)
start: do not consider before this date
done: completion date
For priority: p:0, p:1, p:2, p:3
And a special combination
taskid:<user entered name> and then later they can do waiting:taskid or noteid, after:taskid noteid, blocked-by:taskid to show a todo is clearly linked to another. Noteids are automatically generated behind the scenes for full notes, the taskid is a way to link particular lines in a note with a user generated id.
And the usual @ and # for links and categories

We likely need a special text object type for commentary on other notes. In particular this is aimed at AI proposals an fact checks, which is how they are temporarily stored (or permanently stored, users may sometimes wish to keep them around to reference). Commentary would be linked by ID to a particular note, metadata like when generated and by whom (and general object metadata as described elsewhere). It would be used for revisions and rewrites (until they accepted, if accepted they replace the original note description, possible with the old version moved out into a commentary itself if a user clicks "store old version as commentary" option).
Fact check would have an enum, for example strong evidence, some questionable points, many questionable points, full failure. Commentaries might have a "status" enum, for the fact check that's the assessment of accuracy quality. For a writing quality/grammar check this could be a quality assessment like "needs_rewrite", or "minor_issues"
LLM responses could be an extensible family, not just fact checks. A "devils advocate" call would specifically seek to seek potential flaws
Commentary would be searchable but generally not shown in the standard views until a user has drilled down into the particular note that it is linked to.


How sorting works.
The idea behind sorting is basically a tree hierarchy. Each note or category can refer to its 'prior'. Categories can have a sort order, so that they have a 'prior' for a category as well, this is basically a parent category. Categories should never have notes have as a prior/parent. Notes can point at either another note or a category.
The whole idea behind the "book" view is to gradually sort notes so they are no longer a tree, and instead a continuous manuscript (one branch). Categories are effectively chapters or sections. Each note section has a prior. If the prior for a note is a category it is the start of that section. Most note sections have the ID of the note section it follows.
The prior has an enum for type of relationship from the prior. There are a few main types. "new_paragraph" means there is a standard paragraph gap between (default). "same_paragraph" means these notes are combined sentence to sentence as if in one paragraph, with just a space in between characters. Another type "indented_new_paragraph" is a new paragraph but is idented one level (versus the category section level it is in), it is not recursive. 
A special type is "bullet point" and the related "numbered_list", so if multiple note sections are together with these types, they get formatted as a bulleted list or numbered list as designated. Note that for most notes, having a prior with multiple "children" would create branching, but all bullets/numbered list items sharing a parent are grouped together as a list on a single branch. Having a bullet point type with a bullet point type as a prior would create a sub (indented) bullet point

Categories server two purposes. In general they are a way to linking to a topic.
Categories (hashtags) have their no whitespace name (standard) and a "long format name" which is how they presented as headings (can include whitespace), and also a "heading level" H1 to H6 or more (like paragraph heading weights). Heading weights do not determine position in hierarchy, except as a tie breaker between two with the same parents. Heading weight is mostly a stylistic detail (how much to visually emphasize that category).
When users add a category, it should autocomplete existing categories defined
Categories can be linked in text (as #category) or included in metadata as a loose array
Sorted view with non-primary categories. Say we have the house design notes primarily sorted by a top level category of "trade" (ie electrical, framing) with many notes tagged to the room of the house (ie kitchen) as well. A user in sorted view should be able to filter down to a particular room, and then the sorted view would filter down to just those tagged with that category, and then they could click through a note to see it in full sorted context.

Knowledge packs
Knowledge packs are not so different from complete books. The main difference with a book is that it has a book metadata detail section and is meant to be used as a generally separate package, while a knowledge pack is meant to be loaded into an existing book.
Knowledge packs are really just a metadata for a note. A note could belong to multiple knowledge packs. Categories are not explicitly part of a knowledge pack, but are pulled in by being used in the notes  of a knowledge pack. When first shaping a knowledge pack for export, a UI view would likely allow selecting all in a category for inclusion.
During import of a knowledge pack, a UI view would allow mapping or remapping categories to existing book categories (starting by suggesting any that are very similar to local categories). Import would allow selective import of the knowledge pack, able to discard some items if not desired.
Knowledge packs would include a version. A useful flow might be to load in a new version, overwriting the existing but skipping any import of note ids where that particular note was modified locally by the local user.

A centralized privacy and "locked" file policy needs to be accessible in the UI. This would allow tagging entire categories as private, locked, etc.
Locked files are the concept of files which are not meant to be edited easily. One form of locking would be a unlock delay, so users could add commentary as a proposed rewrite, but have to wait at least a certain amount of time (say, 24 hours) before they can review it being merged in as the official text. Another form of locked file might require the fact check LLM call to confirm it is a certain level of factual before allowing it to be merged. Generally deleting or unlocking files would have the 24 hour confirmation as the default (we are assuming the user is the admin, the idea here is really protecting the user from themselves, so, for example, a slightly delirious late night editing session doesn't corrupt all their notes).



Do we handle multiple style copies of a note?
Need to clarify fact checking with metadata and references
How do end users personalize this?
Design to use as a memory.md
Versions of the same note, commentary, footnotes, what makes sense?
How do we make it clear some items are preferences users need to replace with their own?

Christian and Atheist knowledge packs. The Christian one should be fair, and aim for the kind of Christian that is actually good (unlike the American kind, or the kind that has led, through most of their history, to excessive violence). It should cover the history of its evolution.

User stories:
Users use this to build a better knowledge structure (for their projects or big ideas), to bring out their ideas, fact check them, and then use them as context in LLM calls.
Users can use this to write a book (or at least make most of the detailed outline and then export to final word processor for completion)

Infra:
	Frontend: Typescript using Lexical
	App Core: Rust
	Potential crates to use with Rust: git2, pulldown-cmark, serde_yaml, y-rs, autosurgeon automerge-wasm, candle
	Embeddings: LanceDB or something like fastembed-rs + sqlite-vec. The more manual Sqlite version may work better on WASM?
	Build: both a Tauri build targeting: Google Play, Snapcraft, Microsoft Store, and DMG for Mac (no iOS since we don't have a developer account)
	Build also: a PWA app. The pwa version will have some limitations over the native builds, perhaps not being able to use git, using OPFS which has weaker persistence in some places, and wasm-enabled packages. Generally the PWA and Tauri built apps should share as much code as possible.

	Rust Tauri with Tauri-Specta, also targeting a Rust PWA (no git, uses OPFS, candle ywasm or autosurgeon)
	LanceDB or fastembed-rs + sqlite-vec
	https://github.com/automerge/autosurgeon or Y-rs
	Lexical
	https://github.com/huggingface/candle 

https://github.com/manyougz/velotype/tree/main
https://github.com/pop-os/cosmic-text

Try to keep third-party libraries with clean seams so that if that library would need to be replaced it could be replaced in a straightforward manner (not easy probably, but at least straightforward)

SVG is likely our preferred input image style for "mind palaces". Worth noting that the future "drawing" option will probably create an SVG as well. So the drawing will share this "overlay" note integration tooling with SVG images that are imported (perhaps any imported SVG is just considered a "drawing" data type). Perhaps SVGs can be CRDT tracked, or would that be too messy?
For handling Lat/Long, this should probably be a special type we fit into markdown. So users can use syntax sugar to add lat/long in the middle of a note or table. We should also support adding location (likely lat long based) into the notes metadata as optional, so an entire note could be tagged to a location. It would be neat if the lat/long can be both real Earth lat long or a generic "world" lat/long (and these other "worlds" could just be an image, ie floor plan image for the first floor of a house).
In total, we would be able to have a "map view" that loads map tiles and can show any geo-tagged notes (this map view is a future extension, not built in first pass). We also have drawings (SVG) and images (JPG/PNG/WebP) which can have overlays. For regular images, we will need to assure that the overlays scale appropriately.
