//! Discovery artifacts for the multi-page static-site publish (platform-infra.md "Publishing"):
//! an Atom feed, `sitemap.xml`, and `robots.txt`. All pure `fn(...) -> String` — file-I/O-free and
//! unit-testable; [`crate::app::publish`] decides *whether* to write them (feed/sitemap only when
//! `PublishConfig::base_url` is set; robots.txt always in multi-page modes) and writes the result.

use chrono::{DateTime, Utc};

use crate::publish::escape_html;

/// One chapter's worth of feed input — a chapter is the feed's unit of entry (one per chapter,
/// per the design: a whole-book feed would be too coarse, per-note too noisy).
pub struct ChapterFeedEntry<'a> {
    pub title: &'a str,
    /// Root-relative href to the chapter's page (e.g. `chapters/kitchen.html`, or
    /// `index.html#kitchen` in single-page mode).
    pub href: &'a str,
    /// The most-recently-updated note in the chapter — the feed's `updated` timestamp.
    pub updated: DateTime<Utc>,
}

/// Build `feed.xml` as an Atom feed (RFC 4287 over RSS 2.0: chrono's RFC 3339 output is Atom's
/// native `updated` format). One entry per chapter, newest first.
pub fn render_atom_feed(
    base_url: &str,
    site_title: &str,
    generated_at: DateTime<Utc>,
    mut entries: Vec<ChapterFeedEntry>,
) -> String {
    entries.sort_by(|a, b| b.updated.cmp(&a.updated));

    let mut body = String::new();
    for entry in &entries {
        let link = format!("{base_url}/{}", entry.href);
        body.push_str(&format!(
            "  <entry>\n    <title>{title}</title>\n    <link href=\"{link}\"/>\n    <id>{link}</id>\n    <updated>{updated}</updated>\n  </entry>\n",
            title = escape_html(entry.title),
            updated = entry.updated.to_rfc3339(),
        ));
    }

    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
<feed xmlns=\"http://www.w3.org/2005/Atom\">\n\
  <title>{title}</title>\n\
  <link href=\"{base_url}/\"/>\n\
  <link href=\"{base_url}/feed.xml\" rel=\"self\"/>\n\
  <id>{base_url}/</id>\n\
  <updated>{generated}</updated>\n\
{body}</feed>\n",
        title = escape_html(site_title),
        generated = generated_at.to_rfc3339(),
    )
}

/// One page's worth of sitemap input.
pub struct SitemapEntry<'a> {
    /// Root-relative href of the page.
    pub href: &'a str,
    /// The most-recently-updated note on the page — the entry's `<lastmod>`.
    pub last_modified: DateTime<Utc>,
}

/// Build `sitemap.xml`: every HTML page with a `<lastmod>`.
pub fn render_sitemap(base_url: &str, entries: &[SitemapEntry]) -> String {
    let mut body = String::new();
    for entry in entries {
        body.push_str(&format!(
            "  <url>\n    <loc>{base_url}/{href}</loc>\n    <lastmod>{lastmod}</lastmod>\n  </url>\n",
            href = entry.href,
            lastmod = entry.last_modified.format("%Y-%m-%d"),
        ));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n{body}</urlset>\n"
    )
}

/// Build `robots.txt`. Always allows everything; adds a `Sitemap:` line only when `base_url` is
/// set, since `sitemap.xml` itself is only written in that case.
pub fn render_robots_txt(base_url: Option<&str>) -> String {
    let mut out = "User-agent: *\nAllow: /\n".to_string();
    if let Some(base_url) = base_url {
        out.push_str(&format!("Sitemap: {base_url}/sitemap.xml\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn dt(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap()
    }

    #[test]
    fn atom_feed_orders_entries_newest_first() {
        let feed = render_atom_feed(
            "https://example.com",
            "Field Guide",
            dt(2026, 1, 1),
            vec![
                ChapterFeedEntry {
                    title: "Older",
                    href: "chapters/older.html",
                    updated: dt(2025, 1, 1),
                },
                ChapterFeedEntry {
                    title: "Newer",
                    href: "chapters/newer.html",
                    updated: dt(2025, 6, 1),
                },
            ],
        );
        let newer_pos = feed.find("Newer").unwrap();
        let older_pos = feed.find("Older").unwrap();
        assert!(
            newer_pos < older_pos,
            "newest chapter must come first:\n{feed}"
        );
        assert!(feed.contains("<link href=\"https://example.com/chapters/newer.html\"/>"));
        assert!(feed.contains("xmlns=\"http://www.w3.org/2005/Atom\""));
    }

    #[test]
    fn atom_feed_escapes_titles() {
        let feed = render_atom_feed(
            "https://example.com",
            "A & B",
            dt(2026, 1, 1),
            vec![ChapterFeedEntry {
                title: "<Tricky> & Title",
                href: "c.html",
                updated: dt(2026, 1, 1),
            }],
        );
        assert!(feed.contains("&lt;Tricky&gt; &amp; Title"));
        assert!(feed.contains("A &amp; B"));
    }

    #[test]
    fn sitemap_lists_every_page_with_lastmod() {
        let sitemap = render_sitemap(
            "https://example.com",
            &[
                SitemapEntry {
                    href: "index.html",
                    last_modified: dt(2026, 1, 2),
                },
                SitemapEntry {
                    href: "chapters/a.html",
                    last_modified: dt(2026, 1, 1),
                },
            ],
        );
        assert!(sitemap.contains("<loc>https://example.com/index.html</loc>"));
        assert!(sitemap.contains("<lastmod>2026-01-02</lastmod>"));
        assert!(sitemap.contains("<loc>https://example.com/chapters/a.html</loc>"));
    }

    #[test]
    fn robots_always_allows_and_adds_sitemap_line_only_when_base_url_set() {
        let without = render_robots_txt(None);
        assert!(without.contains("User-agent: *"));
        assert!(without.contains("Allow: /"));
        assert!(!without.contains("Sitemap:"));

        let with = render_robots_txt(Some("https://example.com"));
        assert!(with.contains("Sitemap: https://example.com/sitemap.xml"));
    }
}
