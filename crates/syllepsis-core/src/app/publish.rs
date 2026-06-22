//! Application command surface for publishing & serving (platform-infra.md): export the book as a
//! read-only static site and keep private content out of a GitHub publish.
//!
//! Both operations enforce the privacy policy (privacy-security.md): a private note **or** a note
//! in a private category is excluded from the published site, and the same set is written into the
//! managed `.gitignore` block so a `git push`-style publish never carries it. The full Google Drive
//! backup is unaffected — privacy here is specifically about the *public* release surface.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::CoreResult;
use crate::model::Note;
use crate::sort;
use crate::storage::{layout, Book, NoteStore};

/// Outcome of a static-site publish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishReport {
    /// Absolute path of the written `index.html`.
    pub index_path: String,
    /// Notes included in the published manuscript.
    pub published_notes: usize,
    /// Notes withheld because they (or their category) are private.
    pub excluded_private: usize,
}

/// Outcome of refreshing the private-content git exclusion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitignoreReport {
    /// Book-relative paths now excluded from git (private notes + private category files).
    pub excluded_paths: Vec<String>,
}

/// Render the book's **public** view (private notes and notes in private categories removed) to a
/// single read-only `index.html` under `out_dir`. Returns what was published and what was withheld.
pub fn publish_site(book: &Book, out_dir: &Path) -> CoreResult<PublishReport> {
    let private_categories = private_category_names(book)?;
    // The "active" corpus a publish considers: everything except archived and pending-deletion
    // notes. Each active note is then either published or withheld for privacy, so the two counts
    // partition it exactly.
    let active: Vec<Note> = book
        .store
        .read_all_notes()?
        .into_iter()
        .filter(|n| {
            !n.metadata.lifecycle.archived && n.metadata.lifecycle.marked_for_deletion_at.is_none()
        })
        .collect();
    let active_count = active.len();

    let public: Vec<Note> = active
        .into_iter()
        .filter(|n| is_publishable(n, &private_categories))
        .collect();
    let published_notes = public.len();

    let categories = book
        .store
        .categories()?
        .into_iter()
        .filter(|c| !c.private)
        .collect();

    let items = sort::render(public, categories);
    let markdown = sort::to_markdown(&items);
    let html = crate::publish::render_site(&book.metadata.name, &markdown);

    std::fs::create_dir_all(out_dir)?;
    let index_path = out_dir.join("index.html");
    std::fs::write(&index_path, html)?;

    Ok(PublishReport {
        index_path: index_path.to_string_lossy().into_owned(),
        published_notes,
        excluded_private: active_count.saturating_sub(published_notes),
    })
}

/// Rewrite the managed block of the book's `.gitignore` to exclude every private note file and
/// private category file from a git publish. Idempotent; clearing all private flags removes the
/// block. Returns the excluded paths for display.
pub fn refresh_private_gitignore(book: &Book) -> CoreResult<GitignoreReport> {
    let private_categories = private_category_names(book)?;

    let mut excluded: Vec<String> = Vec::new();
    for note in book.store.read_all_notes()? {
        if note.metadata.lifecycle.private || in_private_category(&note, &private_categories) {
            // Phase-1 flat layout: a note's file is `{id}.md` at the book root.
            excluded.push(layout::note_filename(&note.id));
        }
    }
    for name in &private_categories {
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

/// Names of categories flagged private.
fn private_category_names(book: &Book) -> CoreResult<BTreeSet<String>> {
    Ok(book
        .store
        .categories()?
        .into_iter()
        .filter(|c| c.private)
        .map(|c| c.name)
        .collect())
}

/// A note is publishable if it is visible by default (not private/archived/pending-deletion) and
/// none of its categories are private.
fn is_publishable(note: &Note, private_categories: &BTreeSet<String>) -> bool {
    note.metadata.is_visible_in_default_views() && !in_private_category(note, private_categories)
}

fn in_private_category(note: &Note, private_categories: &BTreeSet<String>) -> bool {
    note.categories
        .iter()
        .any(|c| private_categories.contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::{create_note, update_note};
    use crate::app::lifecycle::{set_category_private, set_note_private};
    use crate::model::{Category, ObjectType, PriorEdge};
    use crate::publish::GITIGNORE_BLOCK_START;

    fn book() -> (tempfile::TempDir, Book) {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path(), "Field Book").unwrap();
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
        let (dir, book) = book();
        book.store.write_category(&Category::new("public")).unwrap();
        book.store.write_category(&Category::new("secret")).unwrap();
        sorted_note(&book, "Open", "public knowledge", &["public"]);
        let hidden = sorted_note(&book, "Hidden", "classified text", &["public"]);
        set_note_private(&book, &hidden, true).unwrap();
        sorted_note(&book, "Vaulted", "in a private category", &["secret"]);
        set_category_private(&book, "secret", true).unwrap();

        let out = dir.path().join("site");
        let report = publish_site(&book, &out).unwrap();

        assert_eq!(report.published_notes, 1);
        assert_eq!(report.excluded_private, 2);
        let html = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(html.contains("public knowledge"));
        assert!(!html.contains("classified text"));
        assert!(!html.contains("in a private category"));
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
}
