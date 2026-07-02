//! Outline-aware split mode: parse tab/space-indented outline documents into categories and
//! small notes, promoting oversized subtrees into their own notes.
//!
//! The parser is deterministic and repair-oriented: real outline documents (years of accreted
//! notes) contain mixed indentation, depth jumps, and mis-indented lines. Every repair is
//! surfaced as a warning so the preview stays reviewable.

use crate::model::PriorKind;

use super::{
    category_preview, dedupe_categories, title_from_text, TextImportBlockKind,
    TextImportCategoryPreview, TextImportOptions, TextImportPreviewItem, TextImportPriorPreview,
    TextImportPriorPreviewTarget,
};

/// Marker bullet appended when a subtree is promoted out into its own note. App-generated, not
/// source text — `keywords::tokenize` strips it so it can't pollute suggestion scoring.
pub(super) const SPLIT_NOTE_MARKER: &str = " (split into its own note)";

/// One line of the outline after indent normalization and depth repair.
#[derive(Debug, Clone)]
struct OutlineLine {
    depth: usize,
    text: String,
    /// A blank line separated this line from the previous non-empty line.
    after_blank: bool,
}

#[derive(Debug, Clone)]
struct OutlineNode {
    text: String,
    after_blank: bool,
    children: Vec<OutlineNode>,
}

impl OutlineNode {
    fn subtree_lines(&self) -> usize {
        1 + self
            .children
            .iter()
            .map(OutlineNode::subtree_lines)
            .sum::<usize>()
    }

    fn subtree_chars(&self) -> usize {
        self.text.chars().count()
            + self
                .children
                .iter()
                .map(OutlineNode::subtree_chars)
                .sum::<usize>()
    }
}

pub fn parse_outline(
    text: &str,
    options: &TextImportOptions,
) -> (
    Vec<TextImportPreviewItem>,
    Vec<TextImportCategoryPreview>,
    Vec<String>,
) {
    let mut warnings = Vec::new();
    let lines = normalize_lines(text, &mut warnings);
    let roots = build_tree(lines, &mut warnings);
    let mut emitter = Emitter::new(options);
    emitter.emit_roots(&roots);
    let promoted = emitter.promoted_count;
    if promoted > 0 {
        warnings.push(format!(
            "Promoted {promoted} large subtree{} into {} own note{}.",
            if promoted == 1 { "" } else { "s" },
            if promoted == 1 { "its" } else { "their" },
            if promoted == 1 { "" } else { "s" },
        ));
    }
    let Emitter {
        mut items,
        categories,
        ..
    } = emitter;
    assign_priors(&mut items);
    for (index, item) in items.iter_mut().enumerate() {
        item.index = index;
    }
    (items, dedupe_categories(categories), warnings)
}

// ── Indent normalization & depth repair ─────────────────────────────────────

fn normalize_lines(text: &str, warnings: &mut Vec<String>) -> Vec<OutlineLine> {
    let raw: Vec<&str> = text.lines().collect();
    let mut tab_lines = 0usize;
    let mut space_lines = 0usize;
    let mut min_space_indent = usize::MAX;
    for line in &raw {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with('\t') {
            tab_lines += 1;
        } else {
            let spaces = line.chars().take_while(|c| *c == ' ').count();
            if spaces > 0 {
                space_lines += 1;
                min_space_indent = min_space_indent.min(spaces);
            }
        }
    }
    if tab_lines > 0 && space_lines > 0 {
        warnings.push(
            "Mixed tab and space indentation detected; depths were normalized per line."
                .to_string(),
        );
    }
    let space_unit = clamp_space_unit(min_space_indent);

    // First pass: raw depth per non-empty line, tracking blank separators.
    let mut lines: Vec<OutlineLine> = Vec::new();
    let mut pending_blank = false;
    for line in &raw {
        if line.trim().is_empty() {
            pending_blank = true;
            continue;
        }
        let depth = line_depth(line, space_unit);
        lines.push(OutlineLine {
            depth,
            text: line.trim().to_string(),
            after_blank: pending_blank,
        });
        pending_blank = false;
    }

    // Second pass: depth repair.
    let raw_depths: Vec<usize> = lines.iter().map(|l| l.depth).collect();
    let mut prev_depth = 0usize;
    for (i, line) in lines.iter_mut().enumerate() {
        let mut depth = line.depth;
        if i == 0 {
            depth = 0;
        } else if depth > prev_depth + 1 {
            warnings.push(format!(
                "Indent jumped from depth {prev_depth} to {depth} at \"{}\"; clamped to depth {}.",
                excerpt(&line.text),
                prev_depth + 1
            ));
            depth = prev_depth + 1;
        } else if depth < prev_depth {
            // A shallow line inside a section whose next line is much deeper is almost
            // certainly mis-indented (its children follow it): pull it back into the section.
            let next_depth = raw_depths.get(i + 1).copied();
            if let Some(next) = next_depth {
                if next >= depth + 2 {
                    let repaired = (next - 1).clamp(1, prev_depth + 1);
                    warnings.push(format!(
                        "\"{}\" looked mis-indented (depth {depth} before depth {next}); repaired into the current section at depth {repaired}.",
                        excerpt(&line.text)
                    ));
                    depth = repaired;
                }
            }
        }
        line.depth = depth;
        prev_depth = depth;
    }
    lines
}

fn clamp_space_unit(min_space_indent: usize) -> usize {
    if min_space_indent == usize::MAX {
        return 4;
    }
    const UNITS: [usize; 4] = [2, 3, 4, 8];
    UNITS
        .into_iter()
        .min_by_key(|unit| unit.abs_diff(min_space_indent))
        .unwrap_or(4)
}

fn line_depth(line: &str, space_unit: usize) -> usize {
    let tabs = line.chars().take_while(|c| *c == '\t').count();
    if tabs > 0 {
        return tabs;
    }
    let spaces = line.chars().take_while(|c| *c == ' ').count();
    spaces / space_unit
}

fn excerpt(text: &str) -> String {
    let compact: String = text.chars().take(48).collect();
    if text.chars().count() > 48 {
        format!("{compact}...")
    } else {
        compact
    }
}

// ── Tree build ──────────────────────────────────────────────────────────────

fn build_tree(lines: Vec<OutlineLine>, _warnings: &mut Vec<String>) -> Vec<OutlineNode> {
    let mut roots: Vec<OutlineNode> = Vec::new();
    // Stack of indices navigating from roots into the currently open branch.
    let mut stack: Vec<usize> = Vec::new();
    for line in lines {
        // Safety clamp: never deeper than one past the open branch.
        let depth = line.depth.min(stack.len());
        stack.truncate(depth);
        let node = OutlineNode {
            text: line.text,
            after_blank: line.after_blank,
            children: Vec::new(),
        };
        let siblings = siblings_at(&mut roots, &stack);
        siblings.push(node);
        let index = siblings.len() - 1;
        stack.push(index);
    }
    roots
}

fn siblings_at<'a>(roots: &'a mut Vec<OutlineNode>, stack: &[usize]) -> &'a mut Vec<OutlineNode> {
    let mut current = roots;
    for &index in stack {
        current = &mut current[index].children;
    }
    current
}

// ── Emission ────────────────────────────────────────────────────────────────

struct Emitter<'a> {
    options: &'a TextImportOptions,
    items: Vec<TextImportPreviewItem>,
    categories: Vec<TextImportCategoryPreview>,
    /// Indices of items that start a category section (get a Category prior).
    section_starts: Vec<usize>,
    promoted_count: usize,
}

impl<'a> Emitter<'a> {
    fn new(options: &'a TextImportOptions) -> Emitter<'a> {
        Emitter {
            options,
            items: Vec::new(),
            categories: Vec::new(),
            section_starts: Vec::new(),
            promoted_count: 0,
        }
    }

    fn emit_roots(&mut self, roots: &[OutlineNode]) {
        let mut loose_group: Vec<&OutlineNode> = Vec::new();
        for root in roots {
            if root.children.is_empty() {
                if self.options.outline_group_loose_lines {
                    if root.after_blank && !loose_group.is_empty() {
                        self.emit_loose_group(&loose_group);
                        loose_group.clear();
                    }
                    loose_group.push(root);
                } else {
                    self.emit_loose_group(&[root]);
                }
                continue;
            }
            if !loose_group.is_empty() {
                self.emit_loose_group(&loose_group);
                loose_group.clear();
            }
            if root.children.len() < self.options.outline_category_min_children
                && !self.exceeds_promotion_thresholds(root)
            {
                // Small subtree: keep it together as one note.
                self.emit_note(root, None, 0, None);
            } else {
                self.emit_section(root);
            }
        }
        if !loose_group.is_empty() {
            self.emit_loose_group(&loose_group);
        }
    }

    fn exceeds_promotion_thresholds(&self, node: &OutlineNode) -> bool {
        node.subtree_lines() >= self.options.outline_promote_min_lines
            || node.subtree_chars() >= self.options.outline_promote_min_chars
    }

    fn emit_loose_group(&mut self, group: &[&OutlineNode]) {
        if group.is_empty() {
            return;
        }
        if group.len() == 1 {
            let text = &group[0].text;
            self.items.push(preview_item(
                title_from_text(text),
                text.clone(),
                TextImportBlockKind::Paragraph,
                None,
                None,
                0,
            ));
            return;
        }
        let body = group
            .iter()
            .map(|node| format!("- {}", node.text))
            .collect::<Vec<_>>()
            .join("\n");
        self.items.push(preview_item(
            title_from_text(&group[0].text),
            body,
            TextImportBlockKind::List,
            None,
            None,
            0,
        ));
    }

    /// Emit a top-level node as a category: the node text names the category and each direct
    /// child becomes a note inside it.
    fn emit_section(&mut self, root: &OutlineNode) {
        let preview = category_preview(&root.text, 2);
        let slug = preview.name.clone();
        self.categories.push(preview);
        self.section_starts.push(self.items.len());
        for child in &root.children {
            self.emit_note(child, Some(slug.clone()), 0, None);
        }
    }

    /// Emit `node` as one note (text on line 1, descendants as bullets), promoting any
    /// large-enough subtree into its own note emitted immediately after this one.
    fn emit_note(
        &mut self,
        node: &OutlineNode,
        category_context: Option<String>,
        ui_depth: u8,
        parent_index: Option<usize>,
    ) -> usize {
        let index = self.items.len();
        // Reserve the slot so promoted children land after their parent.
        self.items.push(preview_item(
            String::new(),
            String::new(),
            TextImportBlockKind::Paragraph,
            category_context.clone(),
            parent_index,
            ui_depth,
        ));

        let mut body_lines = vec![node.text.clone()];
        let mut promoted: Vec<&OutlineNode> = Vec::new();
        self.render_children(&node.children, 1, &mut body_lines, &mut promoted);

        let item = &mut self.items[index];
        item.title = title_from_text(&node.text);
        item.body = body_lines.join("\n");
        item.block_kind = if node.children.is_empty() {
            TextImportBlockKind::Paragraph
        } else {
            TextImportBlockKind::List
        };

        for child in promoted {
            self.promoted_count += 1;
            self.emit_note(child, category_context.clone(), ui_depth + 1, Some(index));
        }
        index
    }

    /// Render `nodes` (at relative depth `rel_depth` under the note root) into bullet lines,
    /// collecting subtrees that should be promoted instead of inlined.
    fn render_children<'n>(
        &self,
        nodes: &'n [OutlineNode],
        rel_depth: usize,
        body_lines: &mut Vec<String>,
        promoted: &mut Vec<&'n OutlineNode>,
    ) {
        let indent = "  ".repeat(rel_depth - 1);
        for node in nodes {
            let should_promote = self.options.outline_promote_max_depth > 0
                && rel_depth <= self.options.outline_promote_max_depth
                && !node.children.is_empty()
                && self.exceeds_promotion_thresholds(node);
            if should_promote {
                body_lines.push(format!(
                    "{indent}- {}{SPLIT_NOTE_MARKER}",
                    node.text
                ));
                promoted.push(node);
            } else {
                body_lines.push(format!("{indent}- {}", node.text));
                self.render_children(&node.children, rel_depth + 1, body_lines, promoted);
            }
        }
    }
}

fn preview_item(
    title: String,
    body: String,
    block_kind: TextImportBlockKind,
    category_context: Option<String>,
    parent_index: Option<usize>,
    depth: u8,
) -> TextImportPreviewItem {
    TextImportPreviewItem {
        index: 0,
        title,
        body,
        block_kind,
        category_context,
        intended_prior: None,
        warnings: Vec::new(),
        categories: Vec::new(),
        parent_index,
        depth,
    }
}

fn assign_priors(items: &mut [TextImportPreviewItem]) {
    let mut section_bounds: Vec<usize> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        // A section starts at the first item carrying a category context different from the
        // previous item's. Promoted notes inherit the same context so they stay in-chain.
        let previous_context = index
            .checked_sub(1)
            .and_then(|i| items[i].category_context.clone());
        if item.category_context.is_some() && item.category_context != previous_context {
            section_bounds.push(index);
        }
    }
    for (index, item) in items.iter_mut().enumerate() {
        item.intended_prior = if section_bounds.contains(&index) {
            item.category_context
                .as_ref()
                .map(|category| TextImportPriorPreview {
                    target: TextImportPriorPreviewTarget::Category,
                    target_label: Some(category.clone()),
                    kind: PriorKind::NewParagraph,
                })
        } else if index == 0 {
            None
        } else {
            Some(TextImportPriorPreview {
                target: TextImportPriorPreviewTarget::PreviousImportedNote,
                target_label: None,
                kind: PriorKind::NewParagraph,
            })
        };
    }
}

#[cfg(test)]
mod tests {
    use super::super::TextImportSplitMode;
    use super::*;

    fn options() -> TextImportOptions {
        TextImportOptions {
            split_mode: TextImportSplitMode::Outline,
            ..TextImportOptions::default()
        }
    }

    /// Fixture modeled on house_notes.md: childless top-level principle lines separated by
    /// blank-line groups, a large section with a promotable subtree, one mis-indented line,
    /// tab indentation throughout.
    const FIXTURE: &str = "\
Simple and cost effective where possible
Durable, long lasting
Highly insulated design

Lots of private spaces
Effective use of space

Sunken garden
\tActs as a security barrier for the big windows
\tRaised garden beds as upper railing
\t\tKeep a gap near the chimney
\tHot tub
\t\tSimple, just a concrete wall
\t\tCover options
\t\t\tRoll-out insulated cover
\t\t\tHard cover with hoist
\t\t\tFloating foam blanket
\t\t\tPergola roof over the tub
\t\t\tSnow load matters for hard covers
MUST have electrical box
\t\tConduit stubbed to the garden wall
\tCatch water in a pond for reuse

Wind turbine on top
\tSmall vertical axis unit
\tNeeds engineering review
";

    fn parse(options: &TextImportOptions) -> (Vec<TextImportPreviewItem>, Vec<String>) {
        let (items, _categories, warnings) = parse_outline(FIXTURE, options);
        (items, warnings)
    }

    #[test]
    fn groups_loose_top_level_lines_between_blank_lines() {
        let (items, _) = parse(&options());
        let first = &items[0];
        assert_eq!(first.title, "Simple and cost effective where possible");
        assert!(first.body.contains("- Durable, long lasting"));
        assert!(first.body.contains("- Highly insulated design"));
        let second = &items[1];
        assert!(second.body.contains("- Lots of private spaces"));
        assert!(second.body.contains("- Effective use of space"));
    }

    #[test]
    fn loose_grouping_toggle_off_gives_one_note_per_line() {
        let mut opts = options();
        opts.outline_group_loose_lines = false;
        let (items, _) = parse(&opts);
        assert_eq!(items[0].body, "Simple and cost effective where possible");
        assert_eq!(items[1].body, "Durable, long lasting");
        assert_eq!(items[2].body, "Highly insulated design");
    }

    #[test]
    fn section_with_enough_children_becomes_category_with_child_notes() {
        let (items, categories, _) = parse_outline(FIXTURE, &options());
        assert!(categories.iter().any(|c| c.name == "sunken-garden"));
        let section_items: Vec<_> = items
            .iter()
            .filter(|item| item.category_context.as_deref() == Some("sunken-garden"))
            .collect();
        assert!(section_items.len() >= 4, "one note per depth-1 child");
        // First note of the section starts the category chain.
        let first = section_items[0];
        assert_eq!(
            first.intended_prior.as_ref().unwrap().target,
            TextImportPriorPreviewTarget::Category
        );
        assert_eq!(
            first.intended_prior.as_ref().unwrap().target_label.as_deref(),
            Some("sunken-garden")
        );
    }

    #[test]
    fn large_subtree_is_promoted_after_its_parent_with_parent_index() {
        let (items, warnings) = parse(&options());
        let hot_tub_parent = items
            .iter()
            .position(|item| item.title.starts_with("Hot tub"))
            .expect("hot tub note");
        let parent = &items[hot_tub_parent];
        // The promoted subtree leaves a marker bullet in the parent body.
        assert!(
            parent.body.contains(SPLIT_NOTE_MARKER),
            "parent body keeps a split marker: {}",
            parent.body
        );
        let promoted = items
            .iter()
            .find(|item| item.parent_index == Some(hot_tub_parent))
            .expect("promoted child note");
        assert_eq!(promoted.depth, 1);
        assert_eq!(promoted.category_context.as_deref(), Some("sunken-garden"));
        // Promotion happens immediately after the parent.
        assert_eq!(promoted.index, hot_tub_parent + 1);
        assert!(warnings.iter().any(|w| w.contains("Promoted")));
    }

    #[test]
    fn mis_indented_top_level_line_is_repaired_into_the_section() {
        let (items, warnings) = parse(&options());
        assert!(
            warnings.iter().any(|w| w.contains("MUST have electrical")),
            "expected repair warning, got: {warnings:?}"
        );
        // The repaired line stays inside the sunken-garden section instead of becoming a
        // standalone top-level note.
        let electrical = items
            .iter()
            .find(|item| item.body.contains("MUST have electrical box"))
            .expect("repaired line present");
        assert_eq!(
            electrical.category_context.as_deref(),
            Some("sunken-garden")
        );
    }

    #[test]
    fn genuine_standalone_section_stays_separate() {
        let (_, categories, _) = parse_outline(FIXTURE, &options());
        // slugify drops the stopword "on".
        assert!(categories.iter().any(|c| c.name == "wind-turbine-top"));
    }

    #[test]
    fn depth_jump_is_clamped_with_warning() {
        let text = "Root\n\tChild\n\t\t\t\tJumped way deeper\n";
        let (items, _categories, warnings) = parse_outline(text, &options());
        assert!(warnings.iter().any(|w| w.contains("clamped")));
        assert_eq!(items.len(), 1);
        assert!(items[0].body.contains("  - Jumped way deeper"));
    }

    #[test]
    fn small_parent_with_single_child_stays_one_note() {
        let text = "Parent line\n\tOnly child\n";
        let (items, categories, _) = parse_outline(text, &options());
        assert_eq!(items.len(), 1);
        assert!(categories.is_empty());
        assert_eq!(items[0].body, "Parent line\n- Only child");
    }

    #[test]
    fn space_indented_outlines_parse_like_tab_indented_ones() {
        let text = "Section head\n  child a\n  child b\n    grandchild\n";
        let (items, categories, _) = parse_outline(text, &options());
        assert!(categories.iter().any(|c| c.name == "section-head"));
        assert_eq!(items.len(), 2);
        assert!(items[1].body.contains("- grandchild"));
    }

    #[test]
    fn mixed_indentation_warns_once() {
        let text = "Head\n\ttab child\n  space child\nHead two\n\ta\n\tb\n";
        let (_items, _categories, warnings) = parse_outline(text, &options());
        assert_eq!(
            warnings
                .iter()
                .filter(|w| w.contains("Mixed tab and space"))
                .count(),
            1
        );
    }

    #[test]
    fn promotion_disabled_when_max_depth_is_zero() {
        let mut opts = options();
        opts.outline_promote_max_depth = 0;
        let (items, _) = parse(&opts);
        assert!(items.iter().all(|item| item.parent_index.is_none()));
        assert!(items
            .iter()
            .all(|item| !item.body.contains(SPLIT_NOTE_MARKER)));
    }
}
