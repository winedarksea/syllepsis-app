//! The shared HTML page shell for the multi-page static-site publish (platform-infra.md
//! "Publishing"): one [`render_page`] renders every page kind (index/TOC, chapter, note) with
//! consistent SEO metadata, dark-mode-aware reading styles, and prev/next navigation, so the
//! per-mode orchestration in [`crate::app::publish`] only has to build a [`PageMeta`] and a body.

use pulldown_cmark::{Event, Options, Parser};

use crate::publish::{escape_html, STYLE};

/// A prev/next/index navigation link.
#[derive(Debug, Clone)]
pub struct NavLink {
    /// Href relative to the page being rendered.
    pub href: String,
    pub title: String,
}

/// Everything a page needs beyond its body HTML: title, SEO metadata, and navigation. Fields that
/// only make sense once the site has a real address (`canonical_url`, `feed_href`) are `None`
/// when the book's `publish.base_url` is unset.
pub struct PageMeta<'a> {
    /// This page's own title (note/chapter title, or the site title on the index page).
    pub page_title: &'a str,
    pub site_title: &'a str,
    /// BCP-47-ish language tag for the `<html lang>` attribute.
    pub lang: &'a str,
    pub description: Option<&'a str>,
    pub author: Option<&'a str>,
    /// True for the index/TOC page (`og:type=website`); false for chapter/note pages (`article`).
    pub is_index: bool,
    /// Absolute canonical URL for this page, present only when `base_url` is configured.
    pub canonical_url: Option<String>,
    /// Root-relative href to `feed.xml`, present only when the feed was written.
    pub feed_href: Option<&'a str>,
    /// Href back to the index page, relative to this page. `None` on the index page itself.
    pub index_href: Option<&'a str>,
    pub prev: Option<NavLink>,
    pub next: Option<NavLink>,
}

/// Render one HTML page: doctype/head with SEO meta, a header linking back to the index, the
/// body, and a footer with prev/next/contents navigation.
pub fn render_page(meta: &PageMeta, body_html: &str) -> String {
    let escaped_page_title = escape_html(meta.page_title);
    let escaped_site_title = escape_html(meta.site_title);
    let title = if meta.is_index {
        escaped_site_title.clone()
    } else {
        format!("{escaped_page_title} · {escaped_site_title}")
    };
    let og_type = if meta.is_index { "website" } else { "article" };

    let mut head = format!(
        "<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<title>{title}</title>\n\
<meta property=\"og:title\" content=\"{escaped_page_title}\">\n\
<meta property=\"og:type\" content=\"{og_type}\">\n"
    );
    if let Some(description) = meta.description {
        let escaped = escape_html(description);
        head.push_str(&format!(
            "<meta name=\"description\" content=\"{escaped}\">\n\
<meta property=\"og:description\" content=\"{escaped}\">\n"
        ));
    }
    if let Some(author) = meta.author {
        head.push_str(&format!(
            "<meta name=\"author\" content=\"{}\">\n",
            escape_html(author)
        ));
    }
    if let Some(canonical) = &meta.canonical_url {
        head.push_str(&format!(
            "<link rel=\"canonical\" href=\"{canonical}\">\n\
<meta property=\"og:url\" content=\"{canonical}\">\n"
        ));
    }
    if let Some(feed_href) = meta.feed_href {
        head.push_str(&format!(
            "<link rel=\"alternate\" type=\"application/atom+xml\" title=\"{escaped_site_title}\" href=\"{feed_href}\">\n"
        ));
    }
    head.push_str(&format!("<style>{STYLE}</style>"));

    let header_href = meta.index_href.unwrap_or(".");
    let header = format!(
        "<header class=\"site-header\"><a class=\"site-title-link\" href=\"{header_href}\">{escaped_site_title}</a></header>\n"
    );

    let mut nav = String::new();
    if meta.prev.is_some() || meta.next.is_some() || meta.index_href.is_some() {
        nav.push_str("<nav class=\"page-nav\">\n");
        if let Some(prev) = &meta.prev {
            nav.push_str(&format!(
                "<a class=\"nav-prev\" href=\"{}\">← {}</a>\n",
                prev.href,
                escape_html(&prev.title)
            ));
        }
        if let Some(index_href) = meta.index_href {
            nav.push_str(&format!(
                "<a class=\"nav-index\" href=\"{index_href}\">Contents</a>\n"
            ));
        }
        if let Some(next) = &meta.next {
            nav.push_str(&format!(
                "<a class=\"nav-next\" href=\"{}\">{} →</a>\n",
                next.href,
                escape_html(&next.title)
            ));
        }
        nav.push_str("</nav>\n");
    }

    format!(
        "<!DOCTYPE html>\n<html lang=\"{lang}\">\n<head>\n{head}\n</head>\n<body>\n{header}\
<main class=\"book\">\n<h1 class=\"book-title\">{escaped_page_title}</h1>\n{body_html}</main>\n\
<footer>{nav}</footer>\n</body>\n</html>\n",
        lang = escape_html(meta.lang),
    )
}

/// Plain-text excerpt of a markdown body for a meta description: strips markdown syntax, then
/// truncates to `max_chars` at a word boundary, appending an ellipsis if truncated.
pub fn plain_text_excerpt(markdown: &str, max_chars: usize) -> String {
    let mut words: Vec<String> = Vec::new();
    for event in Parser::new_ext(markdown, Options::empty()) {
        match event {
            Event::Text(t) | Event::Code(t) => {
                words.extend(t.split_whitespace().map(|w| w.to_string()));
            }
            _ => {}
        }
    }
    let full = words.join(" ");
    if full.chars().count() <= max_chars {
        return full;
    }

    let mut result = String::new();
    for word in &words {
        let candidate_len = if result.is_empty() {
            word.chars().count()
        } else {
            result.chars().count() + 1 + word.chars().count()
        };
        if candidate_len > max_chars {
            break;
        }
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(word);
    }
    if result.is_empty() {
        result = full.chars().take(max_chars).collect();
    }
    format!("{result}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta<'a>(page_title: &'a str, site_title: &'a str) -> PageMeta<'a> {
        PageMeta {
            page_title,
            site_title,
            lang: "en",
            description: None,
            author: None,
            is_index: false,
            canonical_url: None,
            feed_href: None,
            index_href: None,
            prev: None,
            next: None,
        }
    }

    #[test]
    fn renders_title_and_lang() {
        let html = render_page(&meta("Kitchen", "Field Guide"), "<p>hi</p>");
        assert!(html.contains("<html lang=\"en\">"));
        assert!(html.contains("<title>Kitchen · Field Guide</title>"));
        assert!(html.contains("<h1 class=\"book-title\">Kitchen</h1>"));
        assert!(html.contains("<p>hi</p>"));
    }

    #[test]
    fn index_page_title_has_no_separator() {
        let mut m = meta("Field Guide", "Field Guide");
        m.is_index = true;
        let html = render_page(&m, "");
        assert!(html.contains("<title>Field Guide</title>"));
        assert!(html.contains("og:type\" content=\"website\""));
    }

    #[test]
    fn description_and_canonical_only_when_set() {
        let html = render_page(&meta("A", "B"), "");
        assert!(!html.contains("name=\"description\""));
        assert!(!html.contains("rel=\"canonical\""));

        let mut m = meta("A", "B");
        m.description = Some("A note about things.");
        m.canonical_url = Some("https://example.com/a.html".to_string());
        let html = render_page(&m, "");
        assert!(html.contains("<meta name=\"description\" content=\"A note about things.\">"));
        assert!(html.contains("<link rel=\"canonical\" href=\"https://example.com/a.html\">"));
        assert!(html.contains("og:url\" content=\"https://example.com/a.html\""));
    }

    #[test]
    fn feed_link_only_when_set() {
        let html = render_page(&meta("A", "B"), "");
        assert!(!html.contains("atom+xml"));
        let mut m = meta("A", "B");
        m.feed_href = Some("feed.xml");
        let html = render_page(&m, "");
        assert!(html.contains("type=\"application/atom+xml\" title=\"B\" href=\"feed.xml\""));
    }

    #[test]
    fn nav_links_render_when_present() {
        let mut m = meta("A", "B");
        m.index_href = Some("../index.html");
        m.prev = Some(NavLink {
            href: "prev.html".into(),
            title: "Prev Chapter".into(),
        });
        m.next = Some(NavLink {
            href: "next.html".into(),
            title: "Next Chapter".into(),
        });
        let html = render_page(&m, "");
        assert!(html.contains("nav-prev\" href=\"prev.html\">← Prev Chapter"));
        assert!(html.contains("nav-index\" href=\"../index.html\">Contents"));
        assert!(html.contains("nav-next\" href=\"next.html\">Next Chapter →"));
    }

    #[test]
    fn excerpt_returns_full_text_when_short() {
        assert_eq!(plain_text_excerpt("Hello **world**.", 160), "Hello world .");
    }

    #[test]
    fn excerpt_truncates_at_word_boundary_with_ellipsis() {
        let long = "one two three four five six seven eight nine ten";
        let excerpt = plain_text_excerpt(long, 20);
        assert!(excerpt.ends_with('…'));
        assert!(excerpt.len() <= 22); // ~20 chars + ellipsis, not mid-word
        assert!(!excerpt.trim_end_matches('…').ends_with(' '));
    }
}
