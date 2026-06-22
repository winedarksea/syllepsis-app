//! Publishing & serving (platform-infra.md "Import / Export / Serving", privacy-security.md):
//! the read-only static-site export and the git exclusion that keeps **private** content out of a
//! GitHub publish.
//!
//! This module is the pure, book-agnostic core — markdown→HTML rendering of the book view and the
//! managed-`.gitignore`-block string transform — so both are unit-testable without touching the
//! filesystem. The book-aware side (gathering visible notes, writing files) lives in
//! [`crate::app::publish`].

use pulldown_cmark::{html, Options, Parser};

/// Start marker of the app-managed private-exclusion block in `.gitignore`. Everything between the
/// markers is owned by Syllepsis and rewritten on each publish; anything outside is left untouched.
pub const GITIGNORE_BLOCK_START: &str = "# >>> syllepsis private (managed) >>>";
/// End marker of the managed block.
pub const GITIGNORE_BLOCK_END: &str = "# <<< syllepsis private (managed) <<<";

/// Render the book view (already linearized to markdown) as a single self-contained, read-only
/// HTML page. Styling is inlined so the published file needs no external assets — it can be opened
/// directly or served as a static site. `title` is the book name.
pub fn render_site(title: &str, body_markdown: &str) -> String {
    let mut body_html = String::new();
    html::push_html(
        &mut body_html,
        Parser::new_ext(body_markdown, Options::all()),
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

/// Minimal, dependency-free reading styles for the published page.
const STYLE: &str = "\
:root{color-scheme:light dark}\
body{margin:0;background:#faf9f7;color:#1a1a1a;font:1.05rem/1.7 Georgia,'Times New Roman',serif}\
@media(prefers-color-scheme:dark){body{background:#16181c;color:#e7e5e2}}\
.book{max-width:42rem;margin:0 auto;padding:3rem 1.5rem 6rem}\
.book-title{font-size:2.2rem;line-height:1.2;margin:0 0 2rem}\
h1,h2,h3,h4{line-height:1.25;margin:2.4rem 0 .8rem}\
p{margin:0 0 1.1rem}\
code{font-family:ui-monospace,monospace;font-size:.92em}\
blockquote{margin:1.1rem 0;padding-left:1rem;border-left:3px solid #c9c4bd;color:#555}";

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
fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

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
