//! Application command surface for publishing & serving (platform-infra.md): export the book as a
//! multi-page (or single-page) static site and keep private content out of a GitHub publish.
//!
//! Both operations enforce the publish-exclusion capability (privacy-security.md): a note flagged
//! `exclude_from_publish` **or** a note in a category flagged `exclude_from_publish` is withheld
//! from the published site, and the same set is written into the managed `.gitignore` block so a
//! `git push`-style publish never carries it. (The `private` preset turns this capability on along
//! with hiding and search-exclusion, but each is independently controllable.) Being merely `hidden`
//! from the local UI does not withhold a note from the publish. The full Google Drive backup is
//! unaffected — this is specifically about the *public* release surface.
//!
//! Page layout is driven by `book.config.publish.page_mode` ([`PageMode`]): `single` renders
//! everything onto one `index.html` (today's layout, plus working internal links); `per_chapter`
//! (the default) writes one page per top-level category with an index/TOC page; `per_note` writes
//! one page per note. Every mode shares the same [`site::render_page`] shell, the same internal
//! link resolution ([`links::LinkMap`]), and the same manifest-based stale-file cleanup so
//! switching modes on an existing publish folder never leaves orphaned pages behind.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use pulldown_cmark::Options;
use serde::{Deserialize, Serialize};

use crate::app::lifecycle::NoteRef;
use crate::config::PageMode;
use crate::error::CoreResult;
use crate::id;
use crate::model::{Category, Note, ObjectType};
use crate::publish::links::LinkMap;
use crate::publish::{self, feed, links, site};
use crate::sort::{self, ChapterRender, RenderItem, SplitRender};
use crate::storage::{layout, Book, NoteStore};

/// Outcome of a static-site publish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishReport {
    /// Absolute path of the written `index.html`.
    pub index_path: String,
    /// Notes included in the published manuscript.
    pub published_notes: usize,
    /// Notes withheld because they (or their category) are excluded from publish.
    pub excluded_private: usize,
    /// Every page written, `out_dir`-relative (e.g. `index.html`, `chapters/kitchen.html`).
    pub pages_written: Vec<String>,
    /// Notes withheld from this publish, for display (id + title).
    pub withheld_notes: Vec<NoteRef>,
    /// Stale pages from a previous publish (different mode, renamed/removed chapter, ...) that
    /// were removed because a manifest from a prior run was found. `out_dir`-relative.
    pub stale_removed: Vec<String>,
}

/// Outcome of refreshing the private-content git exclusion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitignoreReport {
    /// Book-relative paths now excluded from git (private notes + private category files).
    pub excluded_paths: Vec<String>,
}

/// Facts about a published note snapshotted before it's consumed into the render tree (which
/// drops the title and keeps only what the book view needs).
struct NoteFacts {
    title: String,
    updated: DateTime<Utc>,
    summary: String,
    body: String,
}

impl NoteFacts {
    /// `summary` if non-empty, else a plain-text excerpt of the body.
    fn description(&self) -> String {
        if !self.summary.trim().is_empty() {
            self.summary.clone()
        } else {
            site::plain_text_excerpt(&self.body, 160)
        }
    }
}

/// The manifest of files a prior publish wrote, so a later publish (possibly a different mode)
/// can remove exactly what it no longer writes and nothing else.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    files: Vec<String>,
}

const MANIFEST_FILE: &str = ".syllepsis-publish.json";

/// Render the book's **public** view (publish-excluded notes and notes in publish-excluded
/// categories removed) to `out_dir`, in whatever page mode `book.config.publish.page_mode`
/// selects. Returns what was published, what was withheld, and every page written.
/// `render_code_block(language, code) -> Option<html>` is called for each fenced code block; pass
/// `&|_, _| None` for a plain export with no plugin rendering.
pub fn publish_site(
    book: &Book,
    out_dir: &Path,
    render_code_block: &dyn Fn(&str, &str) -> Option<String>,
) -> CoreResult<PublishReport> {
    let publish_excluded_categories = publish_excluded_category_names(book)?;
    // The "active" corpus a publish considers: everything except archived and pending-deletion
    // notes. Each active note is then either published or withheld for publish-exclusion, so the two
    // partition it exactly. A merely-`hidden` note still publishes — hiding from the local UI is
    // independent from public release.
    let active: Vec<Note> = book
        .store
        .read_all_notes()?
        .into_iter()
        .filter(|n| {
            n.object_type != ObjectType::Commentary
                && !n.metadata.lifecycle.archived
                && n.metadata.lifecycle.marked_for_deletion_at.is_none()
        })
        .collect();
    let active_count = active.len();

    let (public, withheld): (Vec<Note>, Vec<Note>) = active
        .into_iter()
        .partition(|n| is_publishable(n, &publish_excluded_categories));
    let published_notes = public.len();
    let withheld_notes: Vec<NoteRef> = withheld
        .iter()
        .map(|n| NoteRef {
            id: n.id.to_string(),
            title: n.title.clone(),
        })
        .collect();

    let categories: Vec<Category> = book
        .store
        .categories()?
        .into_iter()
        .filter(|c| !c.exclude_from_publish)
        .collect();

    let facts: HashMap<String, NoteFacts> = public
        .iter()
        .map(|n| {
            (
                n.id.as_str().to_string(),
                NoteFacts {
                    title: n.title.clone(),
                    updated: n.metadata.dates.updated,
                    summary: n.summary.clone(),
                    body: n.body.clone(),
                },
            )
        })
        .collect();

    let publish_cfg = book.config.publish.clone();
    let site_title = publish_cfg
        .site_title
        .clone()
        .unwrap_or_else(|| book.metadata.name.clone());
    let lang = book.metadata.language.clone();

    let split = sort::render_split(public, categories);

    std::fs::create_dir_all(out_dir)?;

    let outcome = match publish_cfg.page_mode {
        PageMode::Single => render_single(
            &split,
            &facts,
            &site_title,
            &lang,
            &publish_cfg,
            render_code_block,
        ),
        PageMode::PerChapter => render_per_chapter(
            &split,
            &facts,
            &site_title,
            &lang,
            &publish_cfg,
            render_code_block,
        ),
        PageMode::PerNote => render_per_note(
            &split,
            &facts,
            &site_title,
            &lang,
            &publish_cfg,
            render_code_block,
        ),
    };

    for (rel_path, contents) in &outcome.pages {
        let full = out_dir.join(rel_path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&full, contents)?;
    }

    if let Some(base_url) = &publish_cfg.base_url {
        let sitemap = feed::render_sitemap(base_url, &outcome.sitemap_entries());
        std::fs::write(out_dir.join("sitemap.xml"), sitemap)?;
        let atom =
            feed::render_atom_feed(base_url, &site_title, Utc::now(), outcome.feed_entries());
        std::fs::write(out_dir.join("feed.xml"), atom)?;
    }
    let robots = feed::render_robots_txt(publish_cfg.base_url.as_deref());
    std::fs::write(out_dir.join("robots.txt"), robots)?;

    let mut all_files: Vec<String> = outcome.pages.iter().map(|(p, _)| p.clone()).collect();
    all_files.push("robots.txt".to_string());
    if publish_cfg.base_url.is_some() {
        all_files.push("sitemap.xml".to_string());
        all_files.push("feed.xml".to_string());
    }
    all_files.sort();

    let stale_removed = cleanup_stale_files(out_dir, &all_files)?;

    let index_path = out_dir.join("index.html");

    Ok(PublishReport {
        index_path: index_path.to_string_lossy().into_owned(),
        published_notes,
        excluded_private: active_count.saturating_sub(published_notes),
        pages_written: all_files,
        withheld_notes,
        stale_removed,
    })
}

/// Everything a page-mode renderer produces: the pages to write plus what the feed/sitemap need.
struct RenderOutcome {
    /// `(out_dir-relative path, file contents)`, in write order.
    pages: Vec<(String, String)>,
    /// One entry per chapter: `(title, href, most-recently-updated note in it)`.
    chapters: Vec<(String, String, DateTime<Utc>)>,
}

impl RenderOutcome {
    fn feed_entries(&self) -> Vec<feed::ChapterFeedEntry<'_>> {
        self.chapters
            .iter()
            .map(|(title, href, updated)| feed::ChapterFeedEntry {
                title,
                href,
                updated: *updated,
            })
            .collect()
    }

    fn sitemap_entries(&self) -> Vec<feed::SitemapEntry<'_>> {
        let book_updated = self
            .chapters
            .iter()
            .map(|(_, _, u)| *u)
            .max()
            .unwrap_or_else(Utc::now);
        self.pages
            .iter()
            .map(|(href, _)| {
                // A chapter/note page's own lastmod if we tracked one for it, else the whole
                // book's most-recently-updated note (a reasonable stand-in for TOC/index pages).
                let lastmod = self
                    .chapters
                    .iter()
                    .find(|(_, chapter_href, _)| chapter_href == href)
                    .map(|(_, _, u)| *u)
                    .unwrap_or(book_updated);
                feed::SitemapEntry {
                    href,
                    last_modified: lastmod,
                }
            })
            .collect()
    }
}

/// `cat-<slugified-category-name>` — a stable, collision-free heading anchor for a chapter's
/// section on a shared page (single/per_note index), independent of the auto-assigned heading ids
/// (which dedupe against arbitrary in-body headings and so can't be predicted ahead of render).
fn chapter_anchor(category_name: &str) -> String {
    format!("cat-{}", id::slugify(category_name))
}

/// Markdown for one chapter, with a manual anchor span ahead of its heading (for feed/TOC targets)
/// and per-note anchors within (for internal note links), matching [`sort::to_markdown_anchored`].
fn chapter_markdown(chapter: &ChapterRender) -> String {
    format!(
        "<span id=\"{}\"></span>\n\n{}",
        chapter_anchor(&chapter.category),
        sort::to_markdown_anchored(&chapter.items)
    )
}

fn chapter_description(
    chapter: &ChapterRender,
    facts: &HashMap<String, NoteFacts>,
) -> Option<String> {
    chapter.items.iter().find_map(|item| match item {
        RenderItem::Note(n) => facts.get(n.id.as_str()).map(NoteFacts::description),
        _ => None,
    })
}

fn chapter_updated(chapter: &ChapterRender, facts: &HashMap<String, NoteFacts>) -> DateTime<Utc> {
    chapter
        .items
        .iter()
        .filter_map(|item| match item {
            RenderItem::Note(n) => facts.get(n.id.as_str()).map(|f| f.updated),
            _ => None,
        })
        .max()
        .unwrap_or_else(Utc::now)
}

/// Assign each chapter a unique, filesystem-safe filename: slugified category name, falling back
/// to `chapter-<n>` when the slug is empty, with a `-2`, `-3`, ... suffix on collision.
fn chapter_filenames(chapters: &[ChapterRender]) -> Vec<String> {
    let mut used: HashSet<String> = HashSet::new();
    chapters
        .iter()
        .enumerate()
        .map(|(idx, chapter)| {
            let slug = id::slugify(&chapter.category);
            let base = if slug.is_empty() {
                format!("chapter-{}", idx + 1)
            } else {
                slug
            };
            let mut candidate = base.clone();
            let mut n = 2;
            while used.contains(&candidate) {
                candidate = format!("{base}-{n}");
                n += 1;
            }
            used.insert(candidate.clone());
            candidate
        })
        .collect()
}

// ── single mode ──────────────────────────────────────────────────────────────────────

fn render_single(
    split: &SplitRender,
    facts: &HashMap<String, NoteFacts>,
    site_title: &str,
    lang: &str,
    cfg: &crate::config::PublishConfig,
    render_code_block: &dyn Fn(&str, &str) -> Option<String>,
) -> RenderOutcome {
    let mut link_map = LinkMap::new();
    for chapter in &split.chapters {
        for item in &chapter.items {
            if let RenderItem::Note(n) = item {
                link_map.insert(n.id.ulid().to_string(), "index.html", n.id.as_str());
            }
        }
    }
    for item in &split.loose {
        if let RenderItem::Note(n) = item {
            link_map.insert(n.id.ulid().to_string(), "index.html", n.id.as_str());
        }
    }

    let mut markdown = String::new();
    for chapter in &split.chapters {
        markdown.push_str(&chapter_markdown(chapter));
        markdown.push_str("\n\n");
    }
    markdown.push_str(&sort::to_markdown_anchored(&split.loose));

    let ctx = publish::LinkRewriteContext {
        link_map: &link_map,
        current_page: "index.html",
        mode: PageMode::Single,
    };
    let mut body_html = String::new();
    publish::push_html_for_publish(
        &mut body_html,
        &markdown,
        Options::all(),
        render_code_block,
        Some(&ctx),
    );

    let canonical_url = cfg.base_url.as_ref().map(|b| format!("{b}/index.html"));
    let meta = site::PageMeta {
        page_title: site_title,
        site_title,
        lang,
        description: cfg.description.as_deref(),
        author: cfg.author.as_deref(),
        is_index: true,
        canonical_url,
        feed_href: cfg.base_url.as_ref().map(|_| "feed.xml"),
        index_href: None,
        prev: None,
        next: None,
    };
    let html = site::render_page(&meta, &body_html);

    let chapters = split
        .chapters
        .iter()
        .map(|c| {
            (
                c.heading_text.clone(),
                format!("index.html#{}", chapter_anchor(&c.category)),
                chapter_updated(c, facts),
            )
        })
        .collect();

    RenderOutcome {
        pages: vec![("index.html".to_string(), html)],
        chapters,
    }
}

// ── per_chapter mode ─────────────────────────────────────────────────────────────────

fn render_per_chapter(
    split: &SplitRender,
    facts: &HashMap<String, NoteFacts>,
    site_title: &str,
    lang: &str,
    cfg: &crate::config::PublishConfig,
    render_code_block: &dyn Fn(&str, &str) -> Option<String>,
) -> RenderOutcome {
    let filenames = chapter_filenames(&split.chapters);
    let chapter_pages: Vec<String> = filenames
        .iter()
        .map(|f| format!("chapters/{f}.html"))
        .collect();

    let mut link_map = LinkMap::new();
    for (chapter, page) in split.chapters.iter().zip(&chapter_pages) {
        for item in &chapter.items {
            if let RenderItem::Note(n) = item {
                link_map.insert(n.id.ulid().to_string(), page.clone(), n.id.as_str());
            }
        }
    }
    for item in &split.loose {
        if let RenderItem::Note(n) = item {
            link_map.insert(n.id.ulid().to_string(), "index.html", n.id.as_str());
        }
    }

    let mut pages = Vec::new();

    // Chapter pages, each with prev/next among chapters.
    for (idx, chapter) in split.chapters.iter().enumerate() {
        let page = &chapter_pages[idx];
        let markdown = sort::to_markdown_anchored(&chapter.items);
        let ctx = publish::LinkRewriteContext {
            link_map: &link_map,
            current_page: page,
            mode: PageMode::PerChapter,
        };
        let mut body_html = String::new();
        publish::push_html_for_publish(
            &mut body_html,
            &markdown,
            Options::all(),
            render_code_block,
            Some(&ctx),
        );

        let canonical_url = cfg.base_url.as_ref().map(|b| format!("{b}/{page}"));
        let description = chapter_description(chapter, facts);
        let meta = site::PageMeta {
            page_title: &chapter.heading_text,
            site_title,
            lang,
            description: description.as_deref(),
            author: cfg.author.as_deref(),
            is_index: false,
            canonical_url,
            feed_href: cfg.base_url.as_ref().map(|_| "../feed.xml"),
            index_href: Some("../index.html"),
            prev: (idx > 0).then(|| site::NavLink {
                href: format!("../{}", chapter_pages[idx - 1]),
                title: split.chapters[idx - 1].heading_text.clone(),
            }),
            next: (idx + 1 < split.chapters.len()).then(|| site::NavLink {
                href: format!("../{}", chapter_pages[idx + 1]),
                title: split.chapters[idx + 1].heading_text.clone(),
            }),
        };
        pages.push((page.clone(), site::render_page(&meta, &body_html)));
    }

    // Index/TOC page: chapter links + blurbs, then loose-note content underneath.
    let mut toc = String::from("<nav class=\"toc\"><ul class=\"toc-list\">\n");
    for (chapter, page) in split.chapters.iter().zip(&chapter_pages) {
        toc.push_str(&format!(
            "<li><a href=\"{}\">{}</a>",
            page,
            publish::escape_html(&chapter.heading_text)
        ));
        if let Some(desc) = chapter_description(chapter, facts) {
            toc.push_str(&format!(
                "<p class=\"toc-blurb\">{}</p>",
                publish::escape_html(&desc)
            ));
        }
        toc.push_str("</li>\n");
    }
    toc.push_str("</ul></nav>\n");

    let loose_markdown = sort::to_markdown_anchored(&split.loose);
    let ctx = publish::LinkRewriteContext {
        link_map: &link_map,
        current_page: "index.html",
        mode: PageMode::PerChapter,
    };
    let mut loose_html = String::new();
    publish::push_html_for_publish(
        &mut loose_html,
        &loose_markdown,
        Options::all(),
        render_code_block,
        Some(&ctx),
    );

    let index_body = format!("{toc}{loose_html}");
    let canonical_url = cfg.base_url.as_ref().map(|b| format!("{b}/index.html"));
    let meta = site::PageMeta {
        page_title: site_title,
        site_title,
        lang,
        description: cfg.description.as_deref(),
        author: cfg.author.as_deref(),
        is_index: true,
        canonical_url,
        feed_href: cfg.base_url.as_ref().map(|_| "feed.xml"),
        index_href: None,
        prev: None,
        next: None,
    };
    pages.insert(
        0,
        (
            "index.html".to_string(),
            site::render_page(&meta, &index_body),
        ),
    );

    let chapters = split
        .chapters
        .iter()
        .zip(&chapter_pages)
        .map(|(c, page)| {
            (
                c.heading_text.clone(),
                page.clone(),
                chapter_updated(c, facts),
            )
        })
        .collect();

    RenderOutcome { pages, chapters }
}

// ── per_note mode ────────────────────────────────────────────────────────────────────

fn render_per_note(
    split: &SplitRender,
    facts: &HashMap<String, NoteFacts>,
    site_title: &str,
    lang: &str,
    cfg: &crate::config::PublishConfig,
    render_code_block: &dyn Fn(&str, &str) -> Option<String>,
) -> RenderOutcome {
    // Linear note order: chapters' notes in document order, then loose notes.
    let mut note_ids: Vec<String> = Vec::new();
    for chapter in &split.chapters {
        for item in &chapter.items {
            if let RenderItem::Note(n) = item {
                note_ids.push(n.id.as_str().to_string());
            }
        }
    }
    for item in &split.loose {
        if let RenderItem::Note(n) = item {
            note_ids.push(n.id.as_str().to_string());
        }
    }

    let note_page = |id: &str| format!("notes/{id}.html");

    let mut link_map = LinkMap::new();
    for id in &note_ids {
        link_map.insert(links::ulid_tail(id).to_string(), note_page(id), id.clone());
    }

    let mut pages = Vec::new();

    for item in split
        .chapters
        .iter()
        .flat_map(|c| c.items.iter())
        .chain(split.loose.iter())
    {
        let RenderItem::Note(n) = item else { continue };
        let id_str = n.id.as_str();
        let idx = note_ids.iter().position(|x| x == id_str).unwrap();
        let page = note_page(id_str);

        let markdown = sort::to_markdown_anchored(std::slice::from_ref(item));
        let ctx = publish::LinkRewriteContext {
            link_map: &link_map,
            current_page: &page,
            mode: PageMode::PerNote,
        };
        let mut body_html = String::new();
        publish::push_html_for_publish(
            &mut body_html,
            &markdown,
            Options::all(),
            render_code_block,
            Some(&ctx),
        );

        let title = facts
            .get(id_str)
            .map(|f| f.title.clone())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "(untitled)".to_string());
        let description = facts.get(id_str).map(NoteFacts::description);
        let canonical_url = cfg.base_url.as_ref().map(|b| format!("{b}/{page}"));

        let meta = site::PageMeta {
            page_title: &title,
            site_title,
            lang,
            description: description.as_deref(),
            author: cfg.author.as_deref(),
            is_index: false,
            canonical_url,
            feed_href: cfg.base_url.as_ref().map(|_| "../feed.xml"),
            index_href: Some("../index.html"),
            prev: (idx > 0).then(|| {
                let prev_id = &note_ids[idx - 1];
                site::NavLink {
                    href: format!("../{}", note_page(prev_id)),
                    title: facts
                        .get(prev_id.as_str())
                        .map(|f| f.title.clone())
                        .filter(|t| !t.is_empty())
                        .unwrap_or_else(|| "(untitled)".to_string()),
                }
            }),
            next: (idx + 1 < note_ids.len()).then(|| {
                let next_id = &note_ids[idx + 1];
                site::NavLink {
                    href: format!("../{}", note_page(next_id)),
                    title: facts
                        .get(next_id.as_str())
                        .map(|f| f.title.clone())
                        .filter(|t| !t.is_empty())
                        .unwrap_or_else(|| "(untitled)".to_string()),
                }
            }),
        };
        pages.push((page, site::render_page(&meta, &body_html)));
    }

    // Index/TOC page: grouped by chapter heading, loose notes last.
    let mut toc = String::from("<nav class=\"toc\">\n");
    for chapter in &split.chapters {
        // The `cat-<slug>` id is what the feed/sitemap chapter entries point at
        // (`index.html#cat-<slug>`), so it must exist on the per_note index page too.
        toc.push_str(&format!(
            "<h2 id=\"{}\">{}</h2>\n<ul class=\"toc-list\">\n",
            chapter_anchor(&chapter.category),
            publish::escape_html(&chapter.heading_text)
        ));
        for item in &chapter.items {
            if let RenderItem::Note(n) = item {
                let id_str = n.id.as_str();
                let title = facts
                    .get(id_str)
                    .map(|f| f.title.clone())
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| "(untitled)".to_string());
                toc.push_str(&format!(
                    "<li><a href=\"{}\">{}</a></li>\n",
                    note_page(id_str),
                    publish::escape_html(&title)
                ));
            }
        }
        toc.push_str("</ul>\n");
    }
    if !split.loose.is_empty() {
        toc.push_str("<h2>Uncategorized</h2>\n<ul class=\"toc-list\">\n");
        for item in &split.loose {
            if let RenderItem::Note(n) = item {
                let id_str = n.id.as_str();
                let title = facts
                    .get(id_str)
                    .map(|f| f.title.clone())
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| "(untitled)".to_string());
                toc.push_str(&format!(
                    "<li><a href=\"{}\">{}</a></li>\n",
                    note_page(id_str),
                    publish::escape_html(&title)
                ));
            }
        }
        toc.push_str("</ul>\n");
    }
    toc.push_str("</nav>\n");

    let canonical_url = cfg.base_url.as_ref().map(|b| format!("{b}/index.html"));
    let meta = site::PageMeta {
        page_title: site_title,
        site_title,
        lang,
        description: cfg.description.as_deref(),
        author: cfg.author.as_deref(),
        is_index: true,
        canonical_url,
        feed_href: cfg.base_url.as_ref().map(|_| "feed.xml"),
        index_href: None,
        prev: None,
        next: None,
    };
    pages.insert(
        0,
        ("index.html".to_string(), site::render_page(&meta, &toc)),
    );

    let chapters = split
        .chapters
        .iter()
        .map(|c| {
            (
                c.heading_text.clone(),
                format!("index.html#{}", chapter_anchor(&c.category)),
                chapter_updated(c, facts),
            )
        })
        .collect();

    RenderOutcome { pages, chapters }
}

// ── manifest-based stale-file cleanup ────────────────────────────────────────────────

/// Remove files a prior publish wrote that the current one no longer writes, then persist the new
/// manifest. Never touches a path outside `out_dir` (rejects `..` or absolute entries) and never
/// deletes anything when no manifest exists yet.
fn cleanup_stale_files(out_dir: &Path, new_files: &[String]) -> CoreResult<Vec<String>> {
    let manifest_path = out_dir.join(MANIFEST_FILE);
    let mut removed = Vec::new();

    if let Ok(contents) = std::fs::read_to_string(&manifest_path) {
        if let Ok(previous) = serde_json::from_str::<Manifest>(&contents) {
            let keep: HashSet<&String> = new_files.iter().collect();
            for old in &previous.files {
                if old.contains("..") || Path::new(old).is_absolute() {
                    continue;
                }
                if keep.contains(old) {
                    continue;
                }
                let full = out_dir.join(old);
                if full.is_file() {
                    std::fs::remove_file(&full)?;
                    removed.push(old.clone());
                }
            }
            for dir_name in ["chapters", "notes"] {
                let dir = out_dir.join(dir_name);
                if dir.is_dir() {
                    let empty = std::fs::read_dir(&dir)?.next().is_none();
                    if empty {
                        std::fs::remove_dir(&dir)?;
                    }
                }
            }
        }
    }

    let manifest = Manifest {
        version: 1,
        files: new_files.to_vec(),
    };
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;
    Ok(removed)
}

/// Rewrite the managed block of the book's `.gitignore` to exclude every publish-excluded note file
/// and publish-excluded category file from a git publish. Idempotent; clearing all
/// `exclude_from_publish` flags removes the block. Returns the excluded paths for display.
pub fn refresh_private_gitignore(book: &Book) -> CoreResult<GitignoreReport> {
    let publish_excluded_categories = publish_excluded_category_names(book)?;

    let mut excluded: Vec<String> = Vec::new();
    for note in book.store.read_all_notes()? {
        if note.object_type != ObjectType::Commentary
            && (note.metadata.lifecycle.exclude_from_publish
                || in_publish_excluded_category(&note, &publish_excluded_categories))
        {
            // Phase-1 flat layout: a note's file is `{id}.md` at the book root.
            excluded.push(layout::note_filename(&note.id));
        }
    }
    for name in &publish_excluded_categories {
        excluded.push(format!(
            "{}/{}",
            layout::CATEGORIES_DIR,
            layout::category_filename(name)
        ));
    }
    excluded.sort();
    excluded.dedup();

    let gitignore_path: PathBuf = book.root.join(".gitignore");
    let existing = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
    let updated = crate::publish::apply_managed_gitignore(&existing, &excluded);
    std::fs::write(&gitignore_path, updated)?;

    Ok(GitignoreReport {
        excluded_paths: excluded,
    })
}

/// Names of categories excluded from the publish.
fn publish_excluded_category_names(book: &Book) -> CoreResult<BTreeSet<String>> {
    Ok(book
        .store
        .categories()?
        .into_iter()
        .filter(|c| c.exclude_from_publish)
        .map(|c| c.name)
        .collect())
}

/// A note is publishable if it is not itself publish-excluded, none of its categories are
/// publish-excluded, it is not pending deletion, and it is not commentary. Being merely `hidden`
/// (out of the local default views) does **not** withhold it from the publish.
fn is_publishable(note: &Note, publish_excluded_categories: &BTreeSet<String>) -> bool {
    !note.metadata.lifecycle.exclude_from_publish
        && note.metadata.lifecycle.marked_for_deletion_at.is_none()
        && !in_publish_excluded_category(note, publish_excluded_categories)
        && note.object_type != ObjectType::Commentary
}

fn in_publish_excluded_category(
    note: &Note,
    publish_excluded_categories: &BTreeSet<String>,
) -> bool {
    note.categories
        .iter()
        .any(|c| publish_excluded_categories.contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::{create_note, update_note};
    use crate::app::lifecycle::{
        set_category_private, set_note_exclude_from_publish, set_note_hidden, set_note_private,
    };
    use crate::config::PageMode;
    use crate::model::{Category, ObjectType, PriorEdge};
    use crate::publish::GITIGNORE_BLOCK_START;

    fn book() -> (tempfile::TempDir, Book) {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path(), "Field Book").unwrap();
        (dir, book)
    }

    fn book_with_mode(mode: PageMode) -> (tempfile::TempDir, Book) {
        let (dir, mut book) = book();
        book.config.publish.page_mode = mode;
        (dir, book)
    }

    fn sorted_note(book: &Book, title: &str, body: &str, cats: &[&str]) -> String {
        let mut n = create_note(book, ObjectType::Note, title, None).unwrap();
        n.body = body.into();
        n.categories = cats.iter().map(|c| c.to_string()).collect();
        n.prior = Some(PriorEdge::starts_category(
            cats.first().copied().unwrap_or("intro"),
        ));
        n.sorted = true;
        update_note(book, n).unwrap().id
    }

    #[test]
    fn site_excludes_private_notes_and_private_categories() {
        let (dir, book) = book_with_mode(PageMode::Single);
        book.store.write_category(&Category::new("public")).unwrap();
        book.store.write_category(&Category::new("secret")).unwrap();
        sorted_note(&book, "Open", "public knowledge", &["public"]);
        let hidden = sorted_note(&book, "Hidden", "classified text", &["public"]);
        set_note_private(&book, &hidden, true).unwrap();
        sorted_note(&book, "Vaulted", "in a private category", &["secret"]);
        set_category_private(&book, "secret", true).unwrap();

        let out = dir.path().join("site");
        let report = publish_site(&book, &out, &|_, _| None).unwrap();

        assert_eq!(report.published_notes, 1);
        assert_eq!(report.excluded_private, 2);
        let html = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(html.contains("public knowledge"));
        assert!(!html.contains("classified text"));
        assert!(!html.contains("in a private category"));
        assert_eq!(report.withheld_notes.len(), 2);
    }

    #[test]
    fn hidden_note_still_publishes_but_publish_excluded_note_does_not() {
        let (dir, book) = book_with_mode(PageMode::Single);
        book.store.write_category(&Category::new("public")).unwrap();
        sorted_note(&book, "Open", "public knowledge", &["public"]);
        // Hidden from the local UI, but not publish-excluded → still appears on the public site.
        let hidden = sorted_note(&book, "Tucked", "still public text", &["public"]);
        set_note_hidden(&book, &hidden, true).unwrap();
        // Publish-excluded but otherwise visible → withheld from the public site.
        let withheld = sorted_note(&book, "Withheld", "release-blocked text", &["public"]);
        set_note_exclude_from_publish(&book, &withheld, true).unwrap();

        let out = dir.path().join("site");
        let report = publish_site(&book, &out, &|_, _| None).unwrap();

        assert_eq!(report.published_notes, 2);
        assert_eq!(report.excluded_private, 1);
        let html = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(html.contains("public knowledge"));
        assert!(html.contains("still public text"));
        assert!(!html.contains("release-blocked text"));
    }

    #[test]
    fn gitignore_lists_private_files_and_clears_when_unset() {
        let (_d, book) = book();
        book.store.write_category(&Category::new("secret")).unwrap();
        let n = sorted_note(&book, "Hidden", "x", &["public"]);
        set_note_private(&book, &n, true).unwrap();
        set_category_private(&book, "secret", true).unwrap();

        let report = refresh_private_gitignore(&book).unwrap();
        assert!(report.excluded_paths.iter().any(|p| p.ends_with(".md")));
        assert!(report
            .excluded_paths
            .contains(&"_categories/secret.md".to_string()));

        let gitignore = std::fs::read_to_string(book.root.join(".gitignore")).unwrap();
        assert!(gitignore.contains(GITIGNORE_BLOCK_START));
        // The original gitignore entries survive.
        assert!(gitignore.contains("_derived/"));

        // Un-privating everything clears the managed block.
        set_note_private(&book, &n, false).unwrap();
        set_category_private(&book, "secret", false).unwrap();
        refresh_private_gitignore(&book).unwrap();
        let gitignore = std::fs::read_to_string(book.root.join(".gitignore")).unwrap();
        assert!(!gitignore.contains(GITIGNORE_BLOCK_START));
    }

    #[test]
    fn per_chapter_default_writes_chapter_pages_and_toc_index() {
        let (dir, book) = book(); // default page_mode is PerChapter
        book.store
            .write_category(&Category::new("kitchen"))
            .unwrap();
        // Long enough that the chapter's TOC blurb (a 160-char excerpt) differs from the full body.
        let body = "How the stove works: ".to_string() + &"lights, burners, and knobs. ".repeat(10);
        sorted_note(&book, "Stove", &body, &["kitchen"]);

        let out = dir.path().join("site");
        let report = publish_site(&book, &out, &|_, _| None).unwrap();

        assert!(report
            .pages_written
            .contains(&"chapters/kitchen.html".to_string()));
        let index = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(index.contains("chapters/kitchen.html"));
        assert!(index.contains("toc-blurb")); // a blurb, not the full note
        assert!(!index.contains(&body)); // full content lives only on the chapter page
        let chapter = std::fs::read_to_string(out.join("chapters/kitchen.html")).unwrap();
        assert!(chapter.contains("lights, burners, and knobs"));
    }

    #[test]
    fn per_chapter_puts_loose_notes_on_index_page() {
        let (dir, book) = book();
        // A prior pointing at a note that doesn't exist never becomes a category page — it's a
        // detached branch, rendered loose on the index (see sort::tree's branch-building rules).
        let mut n = create_note(&book, ObjectType::Note, "Loose", None).unwrap();
        n.body = "an unplaced thought".into();
        n.prior = Some(PriorEdge::follows(
            crate::id::NoteId::generate("note", "ghost"),
            crate::model::PriorKind::NewParagraph,
        ));
        n.sorted = true;
        update_note(&book, n).unwrap();

        let out = dir.path().join("site");
        let report = publish_site(&book, &out, &|_, _| None).unwrap();
        assert_eq!(
            report.pages_written,
            vec!["index.html".to_string(), "robots.txt".to_string()]
        );
        let index = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(index.contains("an unplaced thought"));
    }

    #[test]
    fn per_note_mode_writes_one_page_per_note_with_working_internal_link() {
        let (dir, book) = book_with_mode(PageMode::PerNote);
        book.store
            .write_category(&Category::new("kitchen"))
            .unwrap();
        let target_id = sorted_note(&book, "Stove", "the stove", &["kitchen"]);
        let linker_body = format!("see [the stove](syllepsis://note/{target_id})");
        sorted_note(&book, "Oven", &linker_body, &["kitchen"]);

        let out = dir.path().join("site");
        let report = publish_site(&book, &out, &|_, _| None).unwrap();
        assert!(report
            .pages_written
            .iter()
            .any(|p| p.starts_with("notes/") && p.ends_with(".html")));

        let oven_page = out.join(
            report
                .pages_written
                .iter()
                .find(|p| p.contains("oven"))
                .expect("oven page written"),
        );
        let html = std::fs::read_to_string(&oven_page).unwrap();
        // Not the shortest form (both pages share `notes/`), but a correct root-relative href.
        assert!(
            html.contains(&format!("href=\"../notes/{target_id}.html\"")),
            "got: {html}"
        );
    }

    #[test]
    fn single_mode_rewrites_internal_links_and_drops_unresolved_ones() {
        let (dir, book) = book_with_mode(PageMode::Single);
        book.store
            .write_category(&Category::new("kitchen"))
            .unwrap();
        let target_id = sorted_note(&book, "Stove", "the stove", &["kitchen"]);
        let secret_id = sorted_note(&book, "Secret", "hush", &["kitchen"]);
        set_note_exclude_from_publish(&book, &secret_id, true).unwrap();
        let linker_body = format!(
            "see [the stove](syllepsis://note/{target_id}) and [secret](syllepsis://note/{secret_id})"
        );
        sorted_note(&book, "Oven", &linker_body, &["kitchen"]);

        let out = dir.path().join("site");
        publish_site(&book, &out, &|_, _| None).unwrap();
        let html = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(
            html.contains(&format!("href=\"#{target_id}\"")),
            "got: {html}"
        );
        // The dropped link's target leaks nowhere; its inner text survives as plain content.
        assert!(!html.contains(&secret_id));
        assert!(html.contains("secret"));
    }

    #[test]
    fn note_body_opening_with_a_block_still_renders_as_that_block() {
        let (dir, book) = book_with_mode(PageMode::Single);
        book.store
            .write_category(&Category::new("kitchen"))
            .unwrap();
        // Body opens with a sub-heading and a list — block constructs that a leading inline anchor
        // span would have swallowed into a plain paragraph.
        let body = "## Ingredients\n\n- flour\n- water";
        sorted_note(&book, "Bread", body, &["kitchen"]);

        let out = dir.path().join("site");
        publish_site(&book, &out, &|_, _| None).unwrap();
        let html = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(
            html.contains("Ingredients</h2>"),
            "sub-heading must render as a heading: {html}"
        );
        assert!(
            html.contains("<li>flour</li>"),
            "list must render as a list: {html}"
        );
    }

    #[test]
    fn per_note_index_has_chapter_anchors_for_feed_links() {
        let (dir, mut book) = book_with_mode(PageMode::PerNote);
        book.config.publish.base_url = Some("https://example.com".to_string());
        book.store
            .write_category(&Category::new("kitchen"))
            .unwrap();
        sorted_note(&book, "Stove", "the stove", &["kitchen"]);

        let out = dir.path().join("site");
        publish_site(&book, &out, &|_, _| None).unwrap();
        // The feed points at `index.html#cat-kitchen`; that anchor must exist on the index page.
        let index = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(
            index.contains("id=\"cat-kitchen\""),
            "index must carry the chapter anchor the feed links to: {index}"
        );
        let feed = std::fs::read_to_string(out.join("feed.xml")).unwrap();
        assert!(feed.contains("index.html#cat-kitchen"));
    }

    #[test]
    fn manifest_cleanup_removes_pages_from_a_prior_mode_switch() {
        let (dir, mut book) = book_with_mode(PageMode::PerChapter);
        book.store
            .write_category(&Category::new("kitchen"))
            .unwrap();
        sorted_note(&book, "Stove", "the stove", &["kitchen"]);

        let out = dir.path().join("site");
        let first = publish_site(&book, &out, &|_, _| None).unwrap();
        assert!(out.join("chapters/kitchen.html").exists());
        assert!(first.stale_removed.is_empty());

        book.config.publish.page_mode = PageMode::Single;
        let second = publish_site(&book, &out, &|_, _| None).unwrap();
        assert!(!out.join("chapters/kitchen.html").exists());
        assert!(!out.join("chapters").exists());
        assert!(second
            .stale_removed
            .contains(&"chapters/kitchen.html".to_string()));
    }

    #[test]
    fn manifest_absent_deletes_nothing_and_rejects_traversal_paths() {
        let (dir, book) = book_with_mode(PageMode::Single);
        let out = dir.path().join("site");
        std::fs::create_dir_all(&out).unwrap();
        // A stray file that isn't tracked by any manifest must survive.
        std::fs::write(out.join("stray.html"), "keep me").unwrap();

        publish_site(&book, &out, &|_, _| None).unwrap();
        assert!(out.join("stray.html").exists());

        // A manifest entry that tries to escape out_dir must never be deleted-through.
        let evil_manifest = Manifest {
            version: 1,
            files: vec!["../evil.html".to_string()],
        };
        std::fs::write(
            out.join(MANIFEST_FILE),
            serde_json::to_string(&evil_manifest).unwrap(),
        )
        .unwrap();
        let outside = dir.path().join("evil.html");
        std::fs::write(&outside, "do not delete").unwrap();
        publish_site(&book, &out, &|_, _| None).unwrap();
        assert!(outside.exists());
    }

    #[test]
    fn feed_and_sitemap_only_written_when_base_url_set() {
        let (dir, mut book) = book_with_mode(PageMode::PerChapter);
        book.store
            .write_category(&Category::new("kitchen"))
            .unwrap();
        sorted_note(&book, "Stove", "the stove", &["kitchen"]);
        let out = dir.path().join("site");

        publish_site(&book, &out, &|_, _| None).unwrap();
        assert!(!out.join("feed.xml").exists());
        assert!(!out.join("sitemap.xml").exists());
        let robots = std::fs::read_to_string(out.join("robots.txt")).unwrap();
        assert!(!robots.contains("Sitemap:"));

        book.config.publish.base_url = Some("https://example.com".to_string());
        publish_site(&book, &out, &|_, _| None).unwrap();
        assert!(out.join("feed.xml").exists());
        assert!(out.join("sitemap.xml").exists());
        let robots = std::fs::read_to_string(out.join("robots.txt")).unwrap();
        assert!(robots.contains("Sitemap: https://example.com/sitemap.xml"));
    }
}
