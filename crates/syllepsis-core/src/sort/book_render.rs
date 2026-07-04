//! Flatten the [`SortTree`] into the continuous book view (core-concepts.md "The Book View").
//!
//! The structured [`RenderItem`] list is what the frontend renders (it carries the join kind,
//! list depth, and indent flags so the UI can lay each note out correctly). [`to_markdown`]
//! is a best-effort linear export for manuscript output; the structured items are the
//! authoritative form.
//!
//! Grouping rules applied here (from the prior-relationship table):
//! - `same_paragraph` notes flow into the preceding paragraph, space-separated.
//! - `bullet_point` / `numbered_list` notes become list items; a list item whose prior is
//!   itself a list item nests one level deeper (sub-list).
//! - `indented_new_paragraph` is a one-level indent, not recursive.

use serde::{Deserialize, Serialize};

use crate::id::NoteId;
use crate::model::{Category, Note, PriorKind};
use crate::sort::tree::{self, CategoryNode, NoteNode, SortTree};

/// Maximum markdown heading depth; deeper categories clamp to H6.
const MAX_HEADING_LEVEL: u8 = 6;

/// One element of the rendered book, in document order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RenderItem {
    /// A category rendered as a chapter/section heading.
    Heading {
        level: u8,
        text: String,
        category: String,
    },
    /// A note rendered in place with its join/list layout hints.
    Note(RenderedNote),
}

/// A note positioned in the book, with the layout hints the UI needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderedNote {
    pub id: NoteId,
    pub summary: String,
    pub body: String,
    /// How this note joins its prior.
    pub join: PriorKind,
    /// List nesting depth: 0 = not a list item, 1 = top-level list, 2+ = nested.
    pub list_depth: u8,
    /// True for `indented_new_paragraph`.
    pub indented: bool,
    /// True when the list item is numbered rather than bulleted.
    pub numbered: bool,
}

/// Convenience: build the tree and flatten it in one call.
pub fn render(notes: Vec<Note>, categories: Vec<Category>) -> Vec<RenderItem> {
    flatten(&tree::build(notes, categories))
}

/// Flatten a prebuilt tree into ordered render items (chapters, then unattached branches).
pub fn flatten(tree: &SortTree) -> Vec<RenderItem> {
    let mut items = Vec::new();
    for root in &tree.roots {
        flatten_category(root, &mut items);
    }
    // Branches (sorted-but-unplaced clusters) render after the main narrative, no heading.
    flatten_notes(&tree.branches, 0, &mut items);
    items
}

/// One chapter of a split publish: a root category (its heading text as the chapter title) with
/// every item that belongs to it — including nested subcategory headings/notes flattened inline,
/// since a subsection is part of its parent chapter's page, not a page of its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChapterRender {
    /// The root category's hashtag name (used to derive the chapter's filename/id).
    pub category: String,
    /// The root category's display heading text.
    pub heading_text: String,
    pub items: Vec<RenderItem>,
}

/// The book split into publishable chapters plus loose ("branch") notes, for multi-page publish
/// modes. `loose` mirrors what [`flatten`] appends after chapters with no heading of its own.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SplitRender {
    pub chapters: Vec<ChapterRender>,
    pub loose: Vec<RenderItem>,
}

/// Build the tree and split it into one [`ChapterRender`] per root category (subcategories flatten
/// into their parent chapter's item list, keeping their own heading levels) plus loose branch
/// notes. A root category with no notes anywhere in its subtree is skipped entirely — no empty
/// chapter page. Byte-for-byte, `flatten(&tree)` == concatenating every chapter's items with
/// `loose`, so [`render`]'s output does not change.
pub fn render_split(notes: Vec<Note>, categories: Vec<Category>) -> SplitRender {
    let tree = tree::build(notes, categories);
    let chapters = tree
        .roots
        .iter()
        .filter(|root| category_has_notes(root))
        .map(|root| {
            let mut items = Vec::new();
            flatten_category(root, &mut items);
            ChapterRender {
                category: root.category.name.clone(),
                heading_text: root.category.heading_text().to_string(),
                items,
            }
        })
        .collect();

    let mut loose = Vec::new();
    flatten_notes(&tree.branches, 0, &mut loose);

    SplitRender { chapters, loose }
}

/// True if a category subtree (itself or any descendant) has at least one note to publish.
fn category_has_notes(node: &CategoryNode) -> bool {
    !node.notes.is_empty() || node.children.iter().any(category_has_notes)
}

fn flatten_category(node: &CategoryNode, items: &mut Vec<RenderItem>) {
    let level = node.category.heading_level.clamp(1, MAX_HEADING_LEVEL);
    items.push(RenderItem::Heading {
        level,
        text: node.category.heading_text().to_string(),
        category: node.category.name.clone(),
    });
    flatten_notes(&node.notes, 0, items);
    for child in &node.children {
        flatten_category(child, items);
    }
}

/// Depth-first preorder over a note forest. `parent_list_depth` is the list depth of the
/// caller node when that node is itself a list item (0 otherwise), so a list item under a
/// list item nests one deeper.
fn flatten_notes(nodes: &[NoteNode], parent_list_depth: u8, items: &mut Vec<RenderItem>) {
    for node in nodes {
        let is_list = node.join.is_list_item();
        let list_depth = if is_list { parent_list_depth + 1 } else { 0 };
        items.push(RenderItem::Note(RenderedNote {
            id: node.note.id.clone(),
            summary: node.note.summary.clone(),
            body: node.note.body.clone(),
            join: node.join,
            list_depth,
            indented: node.join == PriorKind::IndentedNewParagraph,
            numbered: node.join == PriorKind::NumberedList,
        }));
        // Children inherit this node's list depth only if this node is a list item.
        let child_parent_depth = if is_list { list_depth } else { 0 };
        flatten_notes(&node.children, child_parent_depth, items);
    }
}

/// Best-effort linear markdown for manuscript export.
pub fn to_markdown(items: &[RenderItem]) -> String {
    to_markdown_impl(items, false)
}

/// Identical to [`to_markdown`], but prepends `<span id="{note_id}"></span>` before each note's
/// content so a publish can deep-link to it (`#<note-id>`). Inline HTML passes through
/// pulldown-cmark untouched, including inside `same_paragraph` joins, so it survives rendering.
pub fn to_markdown_anchored(items: &[RenderItem]) -> String {
    to_markdown_impl(items, true)
}

fn to_markdown_impl(items: &[RenderItem], anchored: bool) -> String {
    let mut out = String::new();
    let mut paragraph = String::new();

    for item in items {
        match item {
            RenderItem::Heading { level, text, .. } => {
                flush_paragraph(&mut out, &mut paragraph);
                ensure_blank_separation(&mut out);
                out.push_str(&"#".repeat(*level as usize));
                out.push(' ');
                out.push_str(text);
                out.push_str("\n\n");
            }
            RenderItem::Note(note) => {
                let content = if anchored {
                    format!("{}{}", note_anchor(note), note_content(note))
                } else {
                    note_content(note)
                };
                if note.list_depth > 0 {
                    flush_paragraph(&mut out, &mut paragraph);
                    let indent = "  ".repeat((note.list_depth - 1) as usize);
                    let marker = if note.numbered { "1." } else { "-" };
                    out.push_str(&format!("{indent}{marker} {}\n", single_line(&content)));
                } else if note.join == PriorKind::SameParagraph && !paragraph.is_empty() {
                    paragraph.push(' ');
                    paragraph.push_str(content.trim());
                } else {
                    flush_paragraph(&mut out, &mut paragraph);
                    paragraph = content;
                }
            }
        }
    }
    flush_paragraph(&mut out, &mut paragraph);
    out.trim_end().to_string()
}

/// Zero-width anchor marking a note's start position for deep-linking.
fn note_anchor(note: &RenderedNote) -> String {
    format!("<span id=\"{}\"></span>", note.id.as_str())
}

/// Prefer the full body; fall back to the summary when the body is empty.
fn note_content(note: &RenderedNote) -> String {
    if note.body.trim().is_empty() {
        note.summary.clone()
    } else {
        note.body.clone()
    }
}

fn single_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn flush_paragraph(out: &mut String, paragraph: &mut String) {
    if paragraph.is_empty() {
        return;
    }
    ensure_blank_separation(out);
    out.push_str(paragraph.trim());
    out.push_str("\n\n");
    paragraph.clear();
}

/// Ensure the output ends with a blank line before starting a new block (unless empty).
fn ensure_blank_separation(out: &mut String) {
    if !out.is_empty() && !out.ends_with("\n\n") {
        if out.ends_with('\n') {
            out.push('\n');
        } else {
            out.push_str("\n\n");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ObjectType, PriorEdge};

    fn note(title: &str, body: &str) -> Note {
        let mut n = Note::new(ObjectType::Note, title, "syllepsis_001");
        n.body = body.into();
        n
    }

    #[test]
    fn heading_precedes_section_notes() {
        let cat = Category::new("intro");
        let mut a = note("a", "First sentence.");
        a.prior = Some(PriorEdge::starts_category("intro"));
        let items = render(vec![a], vec![cat]);
        assert!(matches!(items[0], RenderItem::Heading { .. }));
        assert!(matches!(items[1], RenderItem::Note(_)));
        let md = to_markdown(&items);
        assert!(md.starts_with("## intro"));
        assert!(md.contains("First sentence."));
    }

    #[test]
    fn same_paragraph_joins_with_space() {
        let cat = Category::new("c");
        let mut a = note("a", "Hello");
        a.prior = Some(PriorEdge::starts_category("c"));
        let mut b = note("b", "world.");
        b.prior = Some(PriorEdge::follows(a.id.clone(), PriorKind::SameParagraph));
        let md = to_markdown(&render(vec![a, b], vec![cat]));
        assert!(md.contains("Hello world."), "got: {md}");
    }

    #[test]
    fn bullets_group_and_nest() {
        let cat = Category::new("list");
        let mut head = note("head", "Items:");
        head.prior = Some(PriorEdge::starts_category("list"));
        let mut one = note("one", "first");
        one.prior = Some(PriorEdge::follows(head.id.clone(), PriorKind::BulletPoint));
        let mut sub = note("sub", "nested");
        sub.prior = Some(PriorEdge::follows(one.id.clone(), PriorKind::BulletPoint));
        let mut two = note("two", "second");
        two.prior = Some(PriorEdge::follows(head.id.clone(), PriorKind::BulletPoint));

        let items = render(vec![head, one, sub, two], vec![cat]);
        // Collect (content, depth) in render order. Sibling order within the same millisecond
        // is ulid-driven (not asserted here); the invariants are the depth *multiset* and that
        // the nested item directly follows its parent.
        let rendered: Vec<(String, u8)> = items
            .iter()
            .filter_map(|i| match i {
                RenderItem::Note(n) => Some((n.body.clone(), n.list_depth)),
                _ => None,
            })
            .collect();
        let mut depths: Vec<u8> = rendered.iter().map(|(_, d)| *d).collect();
        depths.sort_unstable();
        assert_eq!(depths, vec![0, 1, 1, 2]); // head(0), first(1), second(1), nested(2)

        // "nested" (depth 2) must immediately follow its parent "first" (depth 1).
        let nested_pos = rendered.iter().position(|(c, _)| c == "nested").unwrap();
        assert_eq!(rendered[nested_pos].1, 2);
        assert_eq!(rendered[nested_pos - 1].0, "first");

        let md = to_markdown(&items);
        assert!(md.contains("- first"));
        assert!(md.contains("  - nested"));
        assert!(md.contains("- second"));
    }

    #[test]
    fn render_split_matches_render_when_concatenated() {
        let intro = Category::new("intro");
        let mut a = note("a", "First sentence.");
        a.prior = Some(PriorEdge::starts_category("intro"));
        let recipes = Category::new("recipes");
        let mut b = note("b", "Second sentence.");
        b.prior = Some(PriorEdge::starts_category("recipes"));
        // A branch: prior points at a note that doesn't exist, so it lands in `loose`.
        let mut c = note("c", "Loose sentence.");
        c.prior = Some(PriorEdge::follows(
            NoteId::generate("note", "ghost"),
            PriorKind::NewParagraph,
        ));

        let flat = render(
            vec![a.clone(), b.clone(), c.clone()],
            vec![intro.clone(), recipes.clone()],
        );
        let split = render_split(vec![a, b, c], vec![intro, recipes]);

        assert_eq!(split.chapters.len(), 2);
        let mut reconstructed: Vec<RenderItem> = split
            .chapters
            .iter()
            .flat_map(|c| c.items.clone())
            .collect();
        reconstructed.extend(split.loose.clone());
        assert_eq!(flat, reconstructed);
    }

    #[test]
    fn render_split_skips_categories_with_no_publishable_notes() {
        let empty = Category::new("empty");
        let has_notes = Category::new("has-notes");
        let mut a = note("a", "content");
        a.prior = Some(PriorEdge::starts_category("has-notes"));

        let split = render_split(vec![a], vec![empty, has_notes]);
        assert_eq!(split.chapters.len(), 1);
        assert_eq!(split.chapters[0].category, "has-notes");
    }

    #[test]
    fn render_split_keeps_nested_subcategory_in_parent_chapter() {
        let mut parent = Category::new("parent");
        parent.heading_level = 1;
        let mut child = Category::new("child");
        child.parent = Some("parent".to_string());
        child.heading_level = 2;
        let mut a = note("a", "parent content");
        a.prior = Some(PriorEdge::starts_category("parent"));
        let mut b = note("b", "child content");
        b.prior = Some(PriorEdge::starts_category("child"));

        let split = render_split(vec![a, b], vec![parent, child]);
        assert_eq!(split.chapters.len(), 1);
        let items = &split.chapters[0].items;
        // Parent heading, parent note, child heading, child note — all in one chapter.
        assert!(items
            .iter()
            .any(|i| matches!(i, RenderItem::Heading { category, .. } if category == "child")));
        assert!(items
            .iter()
            .any(|i| matches!(i, RenderItem::Note(n) if n.body == "child content")));
    }

    #[test]
    fn render_split_with_no_categories_puts_sorted_notes_in_loose() {
        // No categories exist, so a note whose prior targets one becomes a branch (loose).
        let mut branch = note("solo", "text");
        branch.prior = Some(PriorEdge::starts_category("nonexistent"));
        let split = render_split(vec![branch], vec![]);
        assert!(split.chapters.is_empty());
        assert_eq!(split.loose.len(), 1);

        // A genuinely unsorted note (no prior) never enters the book view at all.
        let lonely = note("solo2", "text");
        let split = render_split(vec![lonely], vec![]);
        assert!(split.chapters.is_empty());
        assert!(split.loose.is_empty());
    }

    #[test]
    fn to_markdown_anchored_prepends_note_id_spans() {
        let cat = Category::new("intro");
        let mut a = note("a", "Hello");
        a.prior = Some(PriorEdge::starts_category("intro"));
        let id = a.id.clone();
        let items = render(vec![a], vec![cat]);
        let md = to_markdown_anchored(&items);
        assert!(md.contains(&format!("<span id=\"{}\"></span>Hello", id.as_str())));
        // Plain to_markdown has no anchors.
        assert!(!to_markdown(&items).contains("<span id="));
    }

    #[test]
    fn to_markdown_anchored_survives_same_paragraph_join() {
        let cat = Category::new("c");
        let mut a = note("a", "Hello");
        a.prior = Some(PriorEdge::starts_category("c"));
        let mut b = note("b", "world.");
        b.prior = Some(PriorEdge::follows(a.id.clone(), PriorKind::SameParagraph));
        let a_id = a.id.clone();
        let b_id = b.id.clone();
        let items = render(vec![a, b], vec![cat]);
        let md = to_markdown_anchored(&items);
        assert!(md.contains(&format!("<span id=\"{}\"></span>Hello", a_id.as_str())));
        assert!(md.contains(&format!("<span id=\"{}\"></span>world.", b_id.as_str())));
    }
}
