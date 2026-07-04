//! Publishing & serving (platform-infra.md "Import / Export / Serving", privacy-security.md):
//! the read-only static-site export and the git exclusion that keeps **private** content out of a
//! GitHub publish.
//!
//! This module is the pure, book-agnostic core — markdown→HTML rendering of the book view and the
//! managed-`.gitignore`-block string transform — so both are unit-testable without touching the
//! filesystem. The book-aware side (gathering visible notes, writing files) lives in
//! [`crate::app::publish`].

pub mod feed;
pub mod links;
pub mod site;

use pulldown_cmark::{html, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::config::PageMode;
use links::{HeadingIdAllocator, LinkMap};

/// Start marker of the app-managed private-exclusion block in `.gitignore`. Everything between the
/// markers is owned by Syllepsis and rewritten on each publish; anything outside is left untouched.
pub const GITIGNORE_BLOCK_START: &str = "# >>> syllepsis private (managed) >>>";
/// End marker of the managed block.
pub const GITIGNORE_BLOCK_END: &str = "# <<< syllepsis private (managed) <<<";

/// Render the book view (already linearized to markdown) as a single self-contained, read-only
/// HTML page. Styling is inlined so the published file needs no external assets.
pub fn render_site(title: &str, body_markdown: &str) -> String {
    render_site_with_plugins(title, body_markdown, &|_, _| None)
}

/// Like [`render_site`] but routes fenced code blocks through `render_code_block`. If it returns
/// `Some(html)`, that raw HTML is wrapped in host-owned code-block chrome; `None` falls back to
/// standard pulldown_cmark rendering.
pub fn render_site_with_plugins(
    title: &str,
    body_markdown: &str,
    render_code_block: &dyn Fn(&str, &str) -> Option<String>,
) -> String {
    let mut body_html = String::new();
    push_html_with_plugins(
        &mut body_html,
        body_markdown,
        Options::all(),
        render_code_block,
    );
    let escaped_title = escape_html(title);
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<title>{escaped_title}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n\
<main class=\"book\">\n<h1 class=\"book-title\">{escaped_title}</h1>\n{body_html}</main>\n\
</body>\n</html>\n"
    )
}

/// Build a standalone export HTML document (wider layout, slightly different styles from the
/// publish template). Uses Tables + Strikethrough extensions; comments already stripped by caller.
pub fn build_export_html(
    title: &str,
    body_markdown: &str,
    render_code_block: &dyn Fn(&str, &str) -> Option<String>,
) -> String {
    let mut body_html = String::new();
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    push_html_with_plugins(&mut body_html, body_markdown, opts, render_code_block);
    let escaped_title = escape_html(title);
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n\
<meta charset=\"UTF-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n\
<title>{escaped_title}</title>\n\
<style>\n\
body {{ font-family: Georgia, serif; max-width: 800px; margin: 40px auto; padding: 0 24px; line-height: 1.7; color: #1a1a1a; }}\n\
h1, h2, h3, h4 {{ font-family: inherit; margin: 1.4em 0 0.4em; }}\n\
blockquote {{ border-left: 3px solid #ccc; margin: 1em 0; padding-left: 1em; color: #555; }}\n\
code {{ font-family: monospace; background: #f4f4f4; padding: 1px 4px; border-radius: 3px; }}\n\
pre {{ background: #f4f4f4; padding: 12px; border-radius: 4px; overflow-x: auto; }}\n\
</style>\n</head>\n<body>\n<h1>{escaped_title}</h1>\n{body_html}\n</body>\n</html>"
    )
}

/// Walk a pulldown_cmark event stream, intercept fenced code blocks, and route them through
/// `render_code_block(language, code) -> Option<html>`. A `Some` result emits plugin HTML inside a
/// host-owned wrapper; `None` reconstructs the standard `<pre><code>` events. Thin wrapper over
/// [`push_html_for_publish`] with link rewriting and heading ids switched off, so every existing
/// caller (note-preview rendering, the legacy single-file export) keeps today's exact output.
pub fn push_html_with_plugins<'a>(
    output: &mut String,
    markdown: &'a str,
    options: Options,
    render_code_block: &dyn Fn(&str, &str) -> Option<String>,
) {
    push_html_for_publish(output, markdown, options, render_code_block, None);
}

/// What a publish page needs to rewrite internal `syllepsis://note/` links relative to itself.
pub struct LinkRewriteContext<'a> {
    pub link_map: &'a LinkMap,
    /// This page's own site-root-relative path (e.g. `index.html`, `chapters/kitchen.html`).
    pub current_page: &'a str,
    pub mode: PageMode,
}

/// Like [`push_html_with_plugins`], but when `link_ctx` is `Some`, also:
/// - rewrites `syllepsis://note/<id>[#fragment]` link destinations to a relative href resolved
///   against `link_ctx.link_map` ([`links::resolve_link`]); a link to a note that isn't published
///   has its start/end tag events dropped, leaving the plain inner text (privacy-safe — nothing
///   about the excluded note leaks into the markup);
/// - assigns a unique `id` attribute to every heading that doesn't already have one, via
///   [`HeadingIdAllocator`], so headings are stable in-page link targets.
pub fn push_html_for_publish<'a>(
    output: &mut String,
    markdown: &'a str,
    options: Options,
    render_code_block: &dyn Fn(&str, &str) -> Option<String>,
    link_ctx: Option<&LinkRewriteContext>,
) {
    let parser = Parser::new_ext(markdown, options);
    let mut in_code: Option<String> = None; // Some(language) while buffering a fenced block
    let mut code_buf = String::new();
    let mut events: Vec<Event<'a>> = Vec::new();
    let mut heading_start_idx: Option<usize> = None;
    let mut heading_text = String::new();
    let mut heading_ids = HeadingIdAllocator::new();
    let mut dropping_link = false;

    for event in parser {
        if in_code.is_some() {
            match event {
                Event::Text(text) => code_buf.push_str(&text),
                Event::End(TagEnd::CodeBlock) => {
                    let lang = in_code.take().unwrap();
                    if let Some(plugin_html) =
                        render_code_block(&lang, code_buf.trim_end_matches('\n'))
                    {
                        events.push(Event::Html(
                            wrap_plugin_code_block_html(&lang, &plugin_html).into(),
                        ));
                        code_buf.clear();
                    } else {
                        let kind = if lang.is_empty() {
                            CodeBlockKind::Indented
                        } else {
                            CodeBlockKind::Fenced(lang.into())
                        };
                        events.push(Event::Start(Tag::CodeBlock(kind)));
                        events.push(Event::Text(std::mem::take(&mut code_buf).into()));
                        events.push(Event::End(TagEnd::CodeBlock));
                    }
                }
                _ => {} // ignore other events while buffering code
            }
            continue;
        }

        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code = Some(match &kind {
                    CodeBlockKind::Fenced(l) => l.to_string(),
                    CodeBlockKind::Indented => String::new(),
                });
                code_buf.clear();
            }
            Event::Start(Tag::Heading {
                level,
                id,
                classes,
                attrs,
            }) if link_ctx.is_some() => {
                heading_text.clear();
                events.push(Event::Start(Tag::Heading {
                    level,
                    id,
                    classes,
                    attrs,
                }));
                heading_start_idx = Some(events.len() - 1);
            }
            Event::End(TagEnd::Heading(level)) if heading_start_idx.is_some() => {
                let idx = heading_start_idx.take().unwrap();
                if let Event::Start(Tag::Heading {
                    level: heading_level,
                    id,
                    classes,
                    attrs,
                }) = events[idx].clone()
                {
                    let final_id = id.unwrap_or_else(|| heading_ids.assign(&heading_text).into());
                    events[idx] = Event::Start(Tag::Heading {
                        level: heading_level,
                        id: Some(final_id),
                        classes,
                        attrs,
                    });
                }
                events.push(Event::End(TagEnd::Heading(level)));
            }
            Event::Text(text) => {
                if heading_start_idx.is_some() {
                    heading_text.push_str(&text);
                }
                events.push(Event::Text(text));
            }
            Event::Code(text) => {
                if heading_start_idx.is_some() {
                    heading_text.push_str(&text);
                }
                events.push(Event::Code(text));
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let note_link = link_ctx.and_then(|_| links::parse_note_link(&dest_url));
                match (link_ctx, note_link) {
                    (Some(ctx), Some((note_ref, fragment))) => match links::resolve_link(
                        ctx.link_map,
                        ctx.current_page,
                        note_ref,
                        fragment,
                        ctx.mode,
                    ) {
                        Some(href) => events.push(Event::Start(Tag::Link {
                            link_type,
                            dest_url: href.into(),
                            title,
                            id,
                        })),
                        None => dropping_link = true,
                    },
                    _ => events.push(Event::Start(Tag::Link {
                        link_type,
                        dest_url,
                        title,
                        id,
                    })),
                }
            }
            Event::End(TagEnd::Link) => {
                if dropping_link {
                    dropping_link = false;
                } else {
                    events.push(Event::End(TagEnd::Link));
                }
            }
            other => events.push(other),
        }
    }

    html::push_html(output, events.into_iter());
}

fn wrap_plugin_code_block_html(language: &str, plugin_html: &str) -> String {
    format!(
        "<div class=\"syl-plugin-render syl-plugin-render--code-block\" data-language=\"{}\">{}</div>",
        escape_html(language),
        plugin_html
    )
}

/// Minimal, dependency-free reading styles for the published page. `pub(crate)` so
/// [`site::render_page`] can share it rather than drifting a second copy.
pub(crate) const STYLE: &str = "\
:root{color-scheme:light dark}\
body{margin:0;background:#faf9f7;color:#1a1a1a;font:1.05rem/1.7 Georgia,'Times New Roman',serif}\
@media(prefers-color-scheme:dark){body{background:#16181c;color:#e7e5e2}}\
.book{max-width:42rem;margin:0 auto;padding:3rem 1.5rem 6rem}\
.book-title{font-size:2.2rem;line-height:1.2;margin:0 0 2rem}\
h1,h2,h3,h4{line-height:1.25;margin:2.4rem 0 .8rem}\
p{margin:0 0 1.1rem}\
code{font-family:ui-monospace,monospace;font-size:.92em}\
blockquote{margin:1.1rem 0;padding-left:1rem;border-left:3px solid #c9c4bd;color:#555}\
table{border-collapse:collapse;width:100%;margin:1.1rem 0}\
th,td{border:1px solid #c9c4bd;padding:.4rem .6rem;text-align:left}\
img{max-width:100%;height:auto}\
input[type=checkbox]{margin-right:.4rem}\
hr{border:none;border-top:1px solid #c9c4bd;margin:2rem 0}\
a{color:#2a5db0}\
@media(prefers-color-scheme:dark){a{color:#8ab4f8}th,td,blockquote,hr{border-color:#3a3d42}}\
pre{overflow-x:auto;padding:.8rem 1rem;background:rgba(0,0,0,.04);border-radius:6px}\
@media(prefers-color-scheme:dark){pre{background:rgba(255,255,255,.06)}}\
.site-header{max-width:42rem;margin:0 auto;padding:1.2rem 1.5rem 0}\
.site-title-link{font-weight:600;text-decoration:none;color:inherit}\
.page-nav{max-width:42rem;margin:0 auto;padding:1rem 1.5rem 3rem;display:flex;gap:1rem;justify-content:space-between;font-size:.92rem}\
.page-nav a{text-decoration:none}\
.page-nav a:hover{text-decoration:underline}\
.nav-index{margin:0 auto}\
@media print{.site-header,.page-nav{display:none}}";

/// Replace (or insert) the managed private-exclusion block in an existing `.gitignore`, listing
/// `private_paths` (book-relative). Idempotent: re-running with the same paths yields the same
/// file, and the user's own non-managed lines are preserved. An empty `private_paths` removes the
/// block entirely so nothing lingers once everything is un-private'd.
pub fn apply_managed_gitignore(existing: &str, private_paths: &[String]) -> String {
    let mut kept = strip_managed_block(existing);

    if private_paths.is_empty() {
        return ensure_trailing_newline(&kept);
    }

    if !kept.is_empty() && !kept.ends_with('\n') {
        kept.push('\n');
    }
    if !kept.is_empty() {
        kept.push('\n');
    }
    kept.push_str(GITIGNORE_BLOCK_START);
    kept.push('\n');
    for path in private_paths {
        kept.push_str(path);
        kept.push('\n');
    }
    kept.push_str(GITIGNORE_BLOCK_END);
    kept.push('\n');
    kept
}

/// Remove the managed block (and the blank line that precedes it) from a `.gitignore` body.
fn strip_managed_block(existing: &str) -> String {
    let Some(start) = existing.find(GITIGNORE_BLOCK_START) else {
        return existing.to_string();
    };
    let head = existing[..start].trim_end_matches([' ', '\t', '\n', '\r']);
    let tail = match existing[start..].find(GITIGNORE_BLOCK_END) {
        Some(end_rel) => {
            let after = start + end_rel + GITIGNORE_BLOCK_END.len();
            existing[after..].trim_start_matches(['\n', '\r'])
        }
        // Unterminated marker (hand-edited): drop everything from the start marker on.
        None => "",
    };
    if tail.is_empty() {
        head.to_string()
    } else {
        format!("{head}\n{tail}")
    }
}

fn ensure_trailing_newline(text: &str) -> String {
    if text.is_empty() || text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{text}\n")
    }
}

/// Escape the four characters that matter in HTML text/attribute context.
pub(crate) fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_html_with_plugins_leaves_headings_and_links_untouched() {
        let mut html = String::new();
        push_html_with_plugins(
            &mut html,
            "## Chapter\n\n[dead link](syllepsis://note/note-x-01jh5k3q2x9y8w7v6t5s4r3q2p)",
            Options::empty(),
            &|_, _| None,
        );
        // No link_ctx passed → old behavior exactly: no heading id, dead link passes through.
        assert!(html.contains("<h2>Chapter</h2>"));
        assert!(html.contains("href=\"syllepsis://note/note-x-01jh5k3q2x9y8w7v6t5s4r3q2p\""));
    }

    #[test]
    fn publish_pipeline_assigns_heading_ids_when_link_ctx_present() {
        let mut html = String::new();
        let link_map = LinkMap::new();
        let ctx = LinkRewriteContext {
            link_map: &link_map,
            current_page: "index.html",
            mode: PageMode::Single,
        };
        push_html_for_publish(
            &mut html,
            "## My Section\n\nHello.",
            Options::empty(),
            &|_, _| None,
            Some(&ctx),
        );
        assert!(
            html.contains("<h2 id=\"my-section\">My Section</h2>"),
            "got: {html}"
        );
    }

    #[test]
    fn publish_pipeline_dedupes_heading_ids_across_notes_on_one_page() {
        let mut html = String::new();
        let link_map = LinkMap::new();
        let ctx = LinkRewriteContext {
            link_map: &link_map,
            current_page: "index.html",
            mode: PageMode::Single,
        };
        push_html_for_publish(
            &mut html,
            "## Overview\n\nA.\n\n## Overview\n\nB.",
            Options::empty(),
            &|_, _| None,
            Some(&ctx),
        );
        assert!(html.contains("id=\"overview\""));
        assert!(html.contains("id=\"overview-2\""));
    }

    #[test]
    fn publish_pipeline_rewrites_resolved_link_and_drops_unresolved_one() {
        let mut html = String::new();
        let mut link_map = LinkMap::new();
        link_map.insert(
            "01jh5k3q2x9y8w7v6t5s4r3q2p",
            "chapters/kitchen.html",
            "note-target-01jh5k3q2x9y8w7v6t5s4r3q2p",
        );
        let ctx = LinkRewriteContext {
            link_map: &link_map,
            current_page: "index.html",
            mode: PageMode::PerChapter,
        };
        push_html_for_publish(
            &mut html,
            "See [resolved](syllepsis://note/note-target-01jh5k3q2x9y8w7v6t5s4r3q2p) and \
             [excluded](syllepsis://note/note-gone-01jh5k9z8y7x6w5v4t3s2r1q0n) notes.",
            Options::empty(),
            &|_, _| None,
            Some(&ctx),
        );
        assert!(html.contains(
            "href=\"chapters/kitchen.html#note-target-01jh5k3q2x9y8w7v6t5s4r3q2p\">resolved</a>"
        ));
        // Unresolved link: start/end tags dropped, plain text kept, nothing about the target leaks.
        assert_eq!(
            html.matches("<a ").count(),
            1,
            "only the resolved link should be an anchor: {html}"
        );
        assert!(html.contains("excluded"));
        assert!(!html.contains("note-gone"));
    }

    #[test]
    fn renders_markdown_into_a_self_contained_page() {
        let html = render_site("My Book", "## Chapter\n\nHello **world**.");
        assert!(html.contains("<title>My Book</title>"));
        assert!(html.contains("<h2>Chapter</h2>"));
        assert!(html.contains("<strong>world</strong>"));
        // Inlined styles → no external asset references.
        assert!(html.contains("<style>"));
        assert!(!html.contains("href="));
    }

    #[test]
    fn title_is_escaped() {
        let html = render_site("A & B <x>", "");
        assert!(html.contains("A &amp; B &lt;x&gt;"));
    }

    #[test]
    fn plugin_code_blocks_are_wrapped_in_host_owned_markup() {
        let mut html = String::new();
        push_html_with_plugins(
            &mut html,
            "Before\n\n```python\nprint('hello')\n```\n\nAfter",
            Options::empty(),
            &|lang, code| {
                assert_eq!(lang, "python");
                assert_eq!(code, "print('hello')");
                Some(
                    "<pre class=\"plugin-codeblock\"><code>print('hello')</code></pre>".to_string(),
                )
            },
        );

        assert!(
            html.contains(
                "<div class=\"syl-plugin-render syl-plugin-render--code-block\" data-language=\"python\">"
            ),
            "expected host wrapper around plugin HTML, got: {html}"
        );
        assert!(html.contains("<pre class=\"plugin-codeblock\"><code>print('hello')</code></pre>"));
        assert!(html.contains("Before"));
        assert!(html.contains("After"));
    }

    #[test]
    fn long_plugin_code_block_preserves_plugin_html_inside_wrapper() {
        let long_line = "x = 'this_is_a_very_long_python_string_that_should_scroll_inside_the_code_block_instead_of_expanding_the_book_view'";
        let markdown = format!("```python\n{long_line}\n```");
        let mut html = String::new();

        push_html_with_plugins(&mut html, &markdown, Options::empty(), &|lang, code| {
            assert_eq!(lang, "python");
            assert_eq!(code, long_line);
            Some(format!(
                "<pre class=\"plugin-codeblock\"><code>{code}</code></pre>"
            ))
        });

        assert!(html.contains("syl-plugin-render--code-block"));
        assert!(html.contains("data-language=\"python\""));
        assert!(html.contains(long_line));
        assert!(html.contains("<pre class=\"plugin-codeblock\"><code>"));
    }

    #[test]
    fn managed_block_is_inserted_and_idempotent() {
        let base = "_derived/\n_sync/\n_crdt/\n";
        let paths = vec![
            "note-secret-01.md".to_string(),
            "_categories/private.md".to_string(),
        ];
        let once = apply_managed_gitignore(base, &paths);
        assert!(once.contains(GITIGNORE_BLOCK_START));
        assert!(once.contains("note-secret-01.md"));
        assert!(once.starts_with("_derived/")); // user lines preserved up front
                                                // Re-applying the same paths must not duplicate or drift.
        assert_eq!(apply_managed_gitignore(&once, &paths), once);
    }

    #[test]
    fn managed_block_updates_and_clears() {
        let base = "_derived/\n";
        let with = apply_managed_gitignore(base, &["a.md".into()]);
        let changed = apply_managed_gitignore(&with, &["b.md".into()]);
        assert!(changed.contains("b.md"));
        assert!(!changed.contains("a.md"));
        // Clearing removes the block but keeps the user's lines.
        let cleared = apply_managed_gitignore(&changed, &[]);
        assert!(!cleared.contains(GITIGNORE_BLOCK_START));
        assert_eq!(cleared, "_derived/\n");
    }
}
