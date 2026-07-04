//! Internal note-link resolution and heading-id slugging for the static-site publish
//! (platform-infra.md "Publishing"). Pure and file-I/O-free: the app layer
//! ([`crate::app::publish`]) builds a [`LinkMap`] from the notes it is about to write and hands it
//! to the event-stream rewriter in [`crate::publish`].
//!
//! Resolution always keys on the note's **ulid tail**
//! ([`crate::id::NoteId::same_identity`]), never the cosmetic slug, so a link written against an
//! old title still resolves after the note is renamed.
//!
//! Fragment handling differs by [`PageMode`] because a note's content lives at a different
//! granularity in each: in `per_note` mode each note is its own page, so `#fragment` is a
//! page-unique heading slug and is kept as-is; in `single`/`per_chapter` mode many notes share a
//! page, so a bare fragment could collide across notes — the fragment is dropped and the link
//! instead resolves to the note's own anchor (written by
//! [`crate::sort::to_markdown_anchored`]).

use std::collections::HashMap;

use crate::config::PageMode;

/// Where a published note's content lives on the site, keyed by the note's ulid tail.
#[derive(Debug, Clone, Default)]
pub struct LinkMap {
    targets: HashMap<String, LinkTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LinkTarget {
    /// Site-root-relative path of the page containing the note (e.g. `index.html`,
    /// `chapters/kitchen.html`, `notes/note-x-01jh....html`).
    page: String,
    /// The note's canonical id string — matches the `<span id="...">` anchor
    /// [`crate::sort::to_markdown_anchored`] writes, so `#<anchor>` always finds the note within
    /// its page.
    anchor: String,
}

impl LinkMap {
    pub fn new() -> LinkMap {
        LinkMap::default()
    }

    /// Record where a published note's content lives. `ulid` is the note's ulid tail (matching
    /// what [`resolve_link`] looks up), `page` is the site-root-relative page path, and `anchor`
    /// is the note's canonical id string.
    pub fn insert(
        &mut self,
        ulid: impl Into<String>,
        page: impl Into<String>,
        anchor: impl Into<String>,
    ) {
        self.targets.insert(
            ulid.into(),
            LinkTarget {
                page: page.into(),
                anchor: anchor.into(),
            },
        );
    }

    pub fn len(&self) -> usize {
        self.targets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }
}

/// Strip the `syllepsis://note/` scheme from a link destination and split off any `#fragment`.
/// `None` for URLs that aren't an internal note link.
pub fn parse_note_link(dest_url: &str) -> Option<(&str, Option<&str>)> {
    let rest = dest_url.strip_prefix("syllepsis://note/")?;
    Some(match rest.split_once('#') {
        Some((id, fragment)) => (id, Some(fragment)),
        None => (rest, None),
    })
}

/// The ulid tail of a note-id-shaped string — resolution always keys on this, never the cosmetic
/// slug, so a stale-slug link (from before a rename) still resolves. `pub(crate)` so
/// [`crate::app::publish`] can key [`LinkMap`] entries the same way lookups do.
pub(crate) fn ulid_tail(id_str: &str) -> &str {
    id_str.rsplit('-').next().unwrap_or(id_str)
}

/// Resolve a `syllepsis://note/<id>[#fragment]` reference against the map, producing an href
/// relative to `current_page`. `None` means the target note isn't published (excluded, deleted,
/// or never existed) — the caller drops the link and keeps only its inner text.
pub fn resolve_link(
    link_map: &LinkMap,
    current_page: &str,
    id_str: &str,
    fragment: Option<&str>,
    mode: PageMode,
) -> Option<String> {
    let target = link_map.targets.get(ulid_tail(id_str))?;
    let same_page = target.page == current_page;

    Some(match mode {
        PageMode::PerNote => match (same_page, fragment) {
            (true, Some(frag)) => format!("#{frag}"),
            (true, None) => String::new(),
            (false, Some(frag)) => format!("{}#{frag}", relative_href(current_page, &target.page)),
            (false, None) => relative_href(current_page, &target.page),
        },
        PageMode::Single | PageMode::PerChapter => {
            if same_page {
                format!("#{}", target.anchor)
            } else {
                format!(
                    "{}#{}",
                    relative_href(current_page, &target.page),
                    target.anchor
                )
            }
        }
    })
}

/// Build an href from `current_page` to `target_page`, both site-root-relative. Not the
/// shortest possible path — `../` once per directory level of `current_page`, then the full
/// root-relative `target_page` — but that's always correct, and the site is at most two levels
/// deep (`chapters/`, `notes/`).
fn relative_href(current_page: &str, target_page: &str) -> String {
    let depth = current_page.matches('/').count();
    format!("{}{}", "../".repeat(depth), target_page)
}

/// Assigns unique, kebab-case ids to headings on one page, using the same slug function as
/// intra-note section links ([`crate::id::slugify`]) so anchors are consistent site-wide.
/// Collisions (headings on the same page that slugify the same, whether from one note or several)
/// get a `-2`, `-3`, ... suffix.
#[derive(Debug, Default)]
pub struct HeadingIdAllocator {
    seen: HashMap<String, u32>,
}

impl HeadingIdAllocator {
    pub fn new() -> HeadingIdAllocator {
        HeadingIdAllocator::default()
    }

    pub fn assign(&mut self, heading_text: &str) -> String {
        let base = crate::id::slugify(heading_text);
        let base = if base.is_empty() {
            "section".to_string()
        } else {
            base
        };
        let count = self.seen.entry(base.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            base
        } else {
            format!("{base}-{count}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_note_link_with_and_without_fragment() {
        assert_eq!(
            parse_note_link("syllepsis://note/note-hello-01jh5k3q2x9y8w7v6t5s4r3q2p"),
            Some(("note-hello-01jh5k3q2x9y8w7v6t5s4r3q2p", None))
        );
        assert_eq!(
            parse_note_link("syllepsis://note/note-hello-01jh5k3q2x9y8w7v6t5s4r3q2p#section"),
            Some(("note-hello-01jh5k3q2x9y8w7v6t5s4r3q2p", Some("section")))
        );
        assert_eq!(parse_note_link("https://example.com"), None);
    }

    #[test]
    fn resolves_on_ulid_tail_despite_stale_slug() {
        let mut map = LinkMap::new();
        map.insert(
            "01jh5k3q2x9y8w7v6t5s4r3q2p",
            "chapters/kitchen.html",
            "note-fresh-title-01jh5k3q2x9y8w7v6t5s4r3q2p",
        );
        let href = resolve_link(
            &map,
            "index.html",
            "note-old-stale-title-01jh5k3q2x9y8w7v6t5s4r3q2p",
            None,
            PageMode::PerChapter,
        );
        assert_eq!(
            href.as_deref(),
            Some("chapters/kitchen.html#note-fresh-title-01jh5k3q2x9y8w7v6t5s4r3q2p")
        );
    }

    #[test]
    fn unknown_note_resolves_to_none() {
        let map = LinkMap::new();
        assert_eq!(
            resolve_link(
                &map,
                "index.html",
                "note-x-01jh5k3q2x9y8w7v6t5s4r3q2p",
                None,
                PageMode::Single
            ),
            None
        );
    }

    #[test]
    fn single_and_per_chapter_drop_fragment_and_use_note_anchor() {
        let mut map = LinkMap::new();
        map.insert("u1", "index.html", "note-a-u1");
        let href = resolve_link(
            &map,
            "index.html",
            "note-a-u1",
            Some("some-section"),
            PageMode::Single,
        );
        assert_eq!(href.as_deref(), Some("#note-a-u1"));

        let href = resolve_link(
            &map,
            "chapters/other.html",
            "note-a-u1",
            Some("some-section"),
            PageMode::PerChapter,
        );
        assert_eq!(href.as_deref(), Some("../index.html#note-a-u1"));
    }

    #[test]
    fn per_note_keeps_fragment_and_targets_the_note_page() {
        let mut map = LinkMap::new();
        map.insert("u1", "notes/note-a-u1.html", "note-a-u1");
        let href = resolve_link(
            &map,
            "notes/note-b-u2.html",
            "note-a-u1",
            Some("intro"),
            PageMode::PerNote,
        );
        // Not the shortest path (could be "note-a-u1.html"), but a correct one: up to the site
        // root, then the full root-relative page path.
        assert_eq!(href.as_deref(), Some("../notes/note-a-u1.html#intro"));

        // Same-page fragment link stays a bare fragment.
        let href = resolve_link(
            &map,
            "notes/note-a-u1.html",
            "note-a-u1",
            Some("intro"),
            PageMode::PerNote,
        );
        assert_eq!(href.as_deref(), Some("#intro"));
    }

    #[test]
    fn heading_id_allocator_dedupes_with_numeric_suffix() {
        let mut alloc = HeadingIdAllocator::new();
        assert_eq!(alloc.assign("Overview"), "overview");
        assert_eq!(alloc.assign("Overview"), "overview-2");
        assert_eq!(alloc.assign("Overview"), "overview-3");
        assert_eq!(alloc.assign("!!!"), "section");
        assert_eq!(alloc.assign("!!!"), "section-2");
    }
}
