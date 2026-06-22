//! Build the sort tree from prior relationships (core-concepts.md "Sorting Model").
//!
//! Every sorted note points at the note *or category* it follows. Categories point at their
//! parent category, forming nested chapters. This module turns a flat set of notes +
//! categories into a forest:
//!
//! - **roots** — top-level categories (chapters), each with its section notes and subsections.
//! - **branches** — sorted clusters not yet attached to the main narrative (e.g. five items
//!   grouped together but not placed under a chapter).
//! - **unsorted** — notes with no prior; they live in the unsorted queue, not the book.
//!
//! **Sibling order is deterministic without an explicit order field:** sibling categories sort
//! by `(heading_level, name)` (heading weight is the documented tiebreaker); sibling notes
//! sort by **ulid**, which is time-ordered, so children fall in creation order.

use std::collections::{HashMap, HashSet};

use crate::model::{Category, Note, PriorKind, PriorRef};

/// A note plus how it joins its prior, with its successor notes as children.
#[derive(Debug)]
pub struct NoteNode {
    pub note: Note,
    /// How this note joins its prior (paragraph/list/etc.).
    pub join: PriorKind,
    /// Notes whose prior is this note (≥2 means a branch; list items render as one list).
    pub children: Vec<NoteNode>,
}

/// A category (chapter/section) with its opening notes and child categories.
#[derive(Debug)]
pub struct CategoryNode {
    pub category: Category,
    /// Notes whose prior is this category — the section's opening chains.
    pub notes: Vec<NoteNode>,
    pub children: Vec<CategoryNode>,
}

/// The full sorted structure of a book.
#[derive(Debug, Default)]
pub struct SortTree {
    pub roots: Vec<CategoryNode>,
    pub branches: Vec<NoteNode>,
    pub unsorted: Vec<Note>,
}

/// Context shared across the recursive builders (read-only relationship maps + mutable note
/// pool and visited set so each note is placed exactly once).
struct Builder {
    notes: HashMap<String, Note>,
    children_of_note: HashMap<String, Vec<String>>,
    openers_of_category: HashMap<String, Vec<String>>,
    categories: HashMap<String, Category>,
    children_of_category: HashMap<Option<String>, Vec<String>>,
    visited: HashSet<String>,
}

/// Build the sort tree, consuming the notes and categories.
pub fn build(notes: Vec<Note>, categories: Vec<Category>) -> SortTree {
    let mut builder = Builder::new(notes, categories);

    // 1. Category forest, each placing its opener-note chains.
    let root_names = builder
        .children_of_category
        .get(&None)
        .cloned()
        .unwrap_or_default();
    let roots = root_names
        .into_iter()
        .map(|name| builder.build_category(&name))
        .collect();

    // 2. Branches: remaining sorted notes whose prior points outside the remaining set.
    let branches = builder.build_branches();

    // 3. Whatever is left with no prior is unsorted.
    let unsorted = builder
        .notes
        .into_values()
        .filter(|n| n.prior.is_none())
        .collect();

    SortTree {
        roots,
        branches,
        unsorted,
    }
}

impl Builder {
    fn new(notes: Vec<Note>, categories: Vec<Category>) -> Builder {
        let notes: HashMap<String, Note> = notes
            .into_iter()
            .map(|n| (n.id.ulid().to_string(), n))
            .collect();

        let mut children_of_note: HashMap<String, Vec<String>> = HashMap::new();
        let mut openers_of_category: HashMap<String, Vec<String>> = HashMap::new();
        for (ulid, note) in &notes {
            if let Some(edge) = &note.prior {
                match &edge.target {
                    PriorRef::Note(parent) => children_of_note
                        .entry(parent.ulid().to_string())
                        .or_default()
                        .push(ulid.clone()),
                    PriorRef::Category(name) => openers_of_category
                        .entry(name.clone())
                        .or_default()
                        .push(ulid.clone()),
                }
            }
        }
        // ulid sort == chronological order for sibling notes.
        for list in children_of_note.values_mut() {
            list.sort();
        }
        for list in openers_of_category.values_mut() {
            list.sort();
        }

        let mut categories_map = HashMap::new();
        let mut children_of_category: HashMap<Option<String>, Vec<String>> = HashMap::new();
        for category in categories {
            children_of_category
                .entry(category.parent.clone())
                .or_default()
                .push(category.name.clone());
            categories_map.insert(category.name.clone(), category);
        }
        // Sibling categories: heading weight is the tiebreaker, then name for determinism.
        for list in children_of_category.values_mut() {
            list.sort_by(|a, b| {
                let ca = &categories_map[a];
                let cb = &categories_map[b];
                ca.heading_level
                    .cmp(&cb.heading_level)
                    .then_with(|| a.cmp(b))
            });
        }

        Builder {
            notes,
            children_of_note,
            openers_of_category,
            categories: categories_map,
            children_of_category,
            visited: HashSet::new(),
        }
    }

    fn build_category(&mut self, name: &str) -> CategoryNode {
        let category = self
            .categories
            .get(name)
            .cloned()
            .unwrap_or_else(|| Category::new(name));
        let opener_ids = self
            .openers_of_category
            .get(name)
            .cloned()
            .unwrap_or_default();
        let notes = self.build_note_forest(opener_ids);
        let child_names = self
            .children_of_category
            .get(&Some(name.to_string()))
            .cloned()
            .unwrap_or_default();
        let children = child_names
            .into_iter()
            .map(|n| self.build_category(&n))
            .collect();
        CategoryNode {
            category,
            notes,
            children,
        }
    }

    /// Build note nodes for a list of ulids (already sorted), skipping any already placed.
    fn build_note_forest(&mut self, ulids: Vec<String>) -> Vec<NoteNode> {
        ulids
            .into_iter()
            .filter_map(|ulid| self.build_note(&ulid))
            .collect()
    }

    fn build_note(&mut self, ulid: &str) -> Option<NoteNode> {
        if !self.visited.insert(ulid.to_string()) {
            return None; // already placed, or a cycle — never render twice.
        }
        let note = self.notes.remove(ulid)?;
        let join = note.prior.as_ref().map(|e| e.kind).unwrap_or_default();
        let child_ids = self.children_of_note.get(ulid).cloned().unwrap_or_default();
        let children = self.build_note_forest(child_ids);
        Some(NoteNode {
            note,
            join,
            children,
        })
    }

    /// After categories are placed, collect remaining sorted notes into branches. A branch
    /// root is a remaining note whose prior points outside the remaining set (a missing
    /// category, or a parent that was already placed/unsorted/missing).
    fn build_branches(&mut self) -> Vec<NoteNode> {
        let remaining: HashSet<String> = self
            .notes
            .iter()
            .filter(|(_, n)| n.prior.is_some())
            .map(|(ulid, _)| ulid.clone())
            .collect();

        let mut roots: Vec<String> = remaining
            .iter()
            .filter(|ulid| {
                let note = &self.notes[*ulid];
                match &note.prior.as_ref().unwrap().target {
                    PriorRef::Note(parent) => !remaining.contains(parent.ulid()),
                    PriorRef::Category(_) => true, // category missing (else it'd be placed)
                }
            })
            .cloned()
            .collect();
        roots.sort();
        let mut branches = self.build_note_forest(roots);

        // Pure cycles (no external root) — attach by ulid order so nothing is lost.
        let mut leftover: Vec<String> = self
            .notes
            .iter()
            .filter(|(ulid, n)| n.prior.is_some() && !self.visited.contains(*ulid))
            .map(|(ulid, _)| ulid.clone())
            .collect();
        leftover.sort();
        branches.extend(self.build_note_forest(leftover));
        branches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ObjectType, PriorEdge};

    fn note(title: &str) -> Note {
        Note::new(ObjectType::Note, title, "syllepsis_001")
    }

    #[test]
    fn places_section_chain_under_category() {
        let cat = Category::new("intro");
        let mut a = note("a");
        a.prior = Some(PriorEdge::starts_category("intro"));
        let mut b = note("b");
        b.prior = Some(PriorEdge::follows(a.id.clone(), PriorKind::NewParagraph));

        let tree = build(vec![b.clone(), a.clone()], vec![cat]);
        assert_eq!(tree.roots.len(), 1);
        let section = &tree.roots[0];
        assert_eq!(section.notes.len(), 1); // single opener: a
        assert_eq!(section.notes[0].note.id, a.id);
        assert_eq!(section.notes[0].children.len(), 1); // b follows a
        assert_eq!(section.notes[0].children[0].note.id, b.id);
        assert!(tree.unsorted.is_empty());
        assert!(tree.branches.is_empty());
    }

    #[test]
    fn unsorted_notes_stay_out_of_the_tree() {
        let lonely = note("quick capture");
        let tree = build(vec![lonely.clone()], vec![]);
        assert_eq!(tree.unsorted.len(), 1);
        assert!(tree.roots.is_empty());
    }

    #[test]
    fn detached_chain_becomes_a_branch() {
        // a → b, but a's prior points at a non-existent note → not under any category.
        let mut a = note("a");
        let ghost = note("ghost");
        a.prior = Some(PriorEdge::follows(
            ghost.id.clone(),
            PriorKind::NewParagraph,
        ));
        let mut b = note("b");
        b.prior = Some(PriorEdge::follows(a.id.clone(), PriorKind::NewParagraph));

        let tree = build(vec![a.clone(), b.clone()], vec![]);
        assert_eq!(tree.branches.len(), 1);
        assert_eq!(tree.branches[0].note.id, a.id);
        assert_eq!(tree.branches[0].children[0].note.id, b.id);
    }

    #[test]
    fn sibling_notes_order_by_creation() {
        // Two notes share the same category opener slot → branch; order is by ulid (time).
        let cat = Category::new("c");
        let mut first = note("first");
        first.prior = Some(PriorEdge::starts_category("c"));
        let mut second = note("second");
        second.prior = Some(PriorEdge::starts_category("c"));
        // `first` was constructed first, so its ulid sorts earlier.
        let tree = build(vec![second.clone(), first.clone()], vec![cat]);
        let openers = &tree.roots[0].notes;
        assert_eq!(openers.len(), 2);
        assert_eq!(openers[0].note.id, first.id);
        assert_eq!(openers[1].note.id, second.id);
    }
}
