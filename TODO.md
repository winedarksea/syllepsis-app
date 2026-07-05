# TODO
Test the merge, split, and fork note tools

Device storage vs app storage for the app (like Obsidian, able to choose with more permissions for the book)

Need to make a GitHub Actions flow that publishes to syllepsis.org (our domain) a landing page as well as the built installer binaries available for download (likely all hosted on Cloudflare Pages). May also need to build the wasm bundles for the built in plugins.
We intend long term to have build targets for desktop (Windows, Linux, and Mac), as well as Android (both apk and aab, one for uploading to Google Play, the other for users to do manual side loads if desired), and Apple iOS. We do not yet have an apple developer account setup for this, so iOS app does not need to be fully built yet.
The action should run on release, and likely would find the action https://github.com/tauri-apps/tauri-action useful. We have one secret set right already, GDRIVE_CLIENT_SECRET which is the secret for the app id for google drive sync, and can add other secrets as needed

When the user is going across the world map, it shows the coordinates (useful). What might be nice is when a user clicks (perhaps click, drops a temporary pin, then has a 'copy' button), it copies those coordinates (perhaps world / x /y) such that they can paste them exactly into a note's location metadata.

The current "dots" for notes placed on worlds can be difficult to see when the world map/image is colorful.

Add basic OTel logging / telemetry option

Add Obsidian frontmatter metadata mapping in the text importer. Likely includes created/updated which are ISO 8601 strings, tags, possibly alias/status

Consider adding "aliases" to the metadata for the notes.
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

Embeddings need to be switchable. We will have only one bundled version, but we may update that in a year or two. Also if users select another (heavier or lighter) embedding model, it needs to regenerate all embeddings.
Future extension: see if we can get "generate summary" working on images too. (Image to Text LLM)

It's not clear the setting `Unload model after idle (seconds)` is worth it. It might actually be wasting compute to load and reload it (the embedding model is actually fairly small in memory, only 200MB which is fine here). It should maybe only clean up on app exit?

Book view should maybe have a toggle for "show tree". Showing the tree would show for cateogries and notes any other side branches that don't fit the main flow (just showing title and summaries for these).
Book view needs a clear layout on screen size, and no overlap of the table of contents

Tree view and recommended view that sorts in a 'proposed' book order (via embedding or possibly LLM) and then users can drag or confirm the sort to speed up note sorting

Privacy
"leave off managed cloud sync" (OpenDAL managed) capability — deferred, see [docs/sync-backup.md](docs/sync-backup.md#deferred-exclude_from_cloud_sync) for why (both sync engines need to become metadata-aware plus a cloud cleanup pass for already-synced notes).

Review what happens if a user, using git and Loro, was to rollback to an earlier commit version of their notes:  If _crdt/ is deleted or a device does a fresh git clone (no sidecars), reconcile calls new_document(body) (engine.rs:290), creating a Loro doc with independent lineage (new peer/op ids). If two such independently-seeded sidecars for the "same" note are ever merged — which only happens over the cloud-drive _crdt SyncProvider, not git — Loro treats them as two independent whole-body inserts and the merge duplicates/concatenates the entire body

It's prompting for use of the keychain all the time (presumably whenever a cloud sync needs to happen), and it's always three requests back to back. Can't we store the token in some way that we don't need to ask permissions to use it every time? We want it to be secure, but if users have to do this more than once per session (ideally never, after first setup of cloud sync) they will not use the app.

Categories and commentary don't get Loro management

Test knowledge pack import/export, and import of a new version of an existing knowledge pack

Consider aligning the types with ts-rs or tauri-specta. tauri-specta requires annotating all ~149 commands and only helps the Tauri boundary (your REST/MCP servers in rest.rs and mcp.rs wouldn't benefit). ts-rs is a smaller change and helps all three boundaries, but leaves the command-wrapper layer hand-typed.


# GITHUB ACTIONS
These are the plan's explicit manual/secret steps:

cargo tauri signer generate → replace the REPLACE_WITH_TAURI_SIGNER_PUBLIC_KEY placeholder in tauri.conf.json and set TAURI_SIGNING_PRIVATE_KEY(_PASSWORD). ⚠️ createUpdaterArtifacts fails the build until this secret exists — that's the intended PR-2 gate, so don't tag until it's set. I deliberately did not generate your production release key.
Secrets (GDRIVE_CLIENT_SECRET, CLOUDFLARE_*, ANDROID_*) and the Cloudflare Pages project — external setup.
Android: the source-level cfg(target_os="android") gating of secrets.rs/onnx/extism paths and cargo tauri android init need the NDK and iterative compilation — captured step-by-step in docs/android.md.
PR 1 (Desktop CI + Website) — Required before first release tag:

GDRIVE_CLIENT_SECRET — Google Drive OAuth client secret (baked into binaries)
CLOUDFLARE_API_TOKEN — Cloudflare Pages API token (scoped: Cloudflare Pages: Edit)
CLOUDFLARE_ACCOUNT_ID — Your Cloudflare account ID
PR 2 (Auto-updater) — Required before this PR merges (else createUpdaterArtifacts fails the build):
4. TAURI_SIGNING_PRIVATE_KEY — from cargo tauri signer generate
5. TAURI_SIGNING_PRIVATE_KEY_PASSWORD — password for the key above

PR 3 (Android) — Required for Android builds:
6. ANDROID_KEYSTORE_BASE64 — base64-encoded .jks upload keystore
7. ANDROID_KEYSTORE_PASSWORD — keystore password
8. ANDROID_KEY_ALIAS — key alias (e.g., "upload")
9. ANDROID_KEY_PASSWORD — key password

Optional (Add when code-signing certs are ready):
10. APPLE_CERTIFICATE — macOS signing certificate (base64)
11. APPLE_CERTIFICATE_PASSWORD — certificate password
12. APPLE_SIGNING_IDENTITY — identity string
13. APPLE_ID — Apple ID email
14. APPLE_PASSWORD — Apple ID app-specific password
15. APPLE_TEAM_ID — Apple Team ID
