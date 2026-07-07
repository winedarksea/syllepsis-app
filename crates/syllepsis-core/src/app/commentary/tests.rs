use super::*;
use crate::model::ObjectType;

fn book() -> (tempfile::TempDir, Book) {
    let dir = tempfile::tempdir().unwrap();
    let book = Book::create(dir.path(), "Test").unwrap();
    (dir, book)
}

fn note(book: &Book, body: &str) -> Note {
    let mut note = book.new_note(ObjectType::Note, "Parent").unwrap();
    note.body = body.to_string();
    book.save_note(&note).unwrap();
    note
}

#[test]
fn commentary_is_stored_outside_the_normal_note_scan() {
    let (_dir, book) = book();
    let parent = note(&book, "body");

    create_commentary(
        &book,
        parent.id.as_str(),
        CommentaryKind::Comment,
        "margin note",
    )
    .unwrap();

    assert_eq!(book.store.read_all_notes().unwrap().len(), 1);
    let commentary = book.read_all_commentary_notes().unwrap();
    assert_eq!(commentary.len(), 1);
    assert_eq!(
        commentary[0].commentary.as_ref().unwrap().parent_note_id,
        parent.id
    );
}

#[test]
fn clean_body_proposal_applies_and_moves_to_trash() {
    let (_dir, book) = book();
    let parent = note(&book, "old body");
    let created = create_commentary(
        &book,
        parent.id.as_str(),
        CommentaryKind::Proposal,
        "new body",
    )
    .unwrap();

    let updated = apply_commentary(&book, &created.id, ApplyCommentaryOptions::default()).unwrap();

    assert_eq!(updated.body, "new body");
    let commentary = book
        .read_commentary_note(&NoteId::parse(&created.id).unwrap())
        .unwrap();
    assert_eq!(
        commentary.commentary.as_ref().unwrap().status,
        CommentaryStatus::Merged
    );
    assert!(commentary
        .metadata
        .lifecycle
        .marked_for_deletion_at
        .is_some());
}

#[test]
fn dismiss_and_pin_update_commentary_lifecycle() {
    let (_dir, book) = book();
    let parent = note(&book, "body");
    let dismissed = create_commentary(
        &book,
        parent.id.as_str(),
        CommentaryKind::Comment,
        "discard",
    )
    .unwrap();
    dismiss_commentary(&book, &dismissed.id).unwrap();
    let discarded = book
        .read_commentary_note(&NoteId::parse(&dismissed.id).unwrap())
        .unwrap();
    assert_eq!(
        discarded.commentary.as_ref().unwrap().status,
        CommentaryStatus::Dismissed
    );

    let pinned =
        create_commentary(&book, parent.id.as_str(), CommentaryKind::Comment, "keep").unwrap();
    let pinned = pin_commentary(&book, &pinned.id).unwrap();
    let meta = pinned.commentary.unwrap();
    assert_eq!(meta.status, CommentaryStatus::Pinned);
    assert_eq!(meta.kind, CommentaryKind::Footnote);
}

#[test]
fn commentary_stays_linked_when_parent_slug_changes() {
    let (_dir, book) = book();
    let mut parent = note(&book, "body");
    let original_id = parent.id.clone();
    create_commentary(
        &book,
        original_id.as_str(),
        CommentaryKind::Comment,
        "margin note",
    )
    .unwrap();

    parent.retitle("Renamed Parent");
    book.save_note(&parent).unwrap();

    let listed = list_commentary(&book, parent.id.as_str(), false).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].body, "margin note");
    assert!(listed[0].metadata.parent_note_id.same_identity(&parent.id));
}

#[test]
fn parent_commentary_deletion_uses_stable_identity() {
    let (_dir, book) = book();
    let mut parent = note(&book, "body");
    let original_id = parent.id.clone();
    create_commentary(
        &book,
        original_id.as_str(),
        CommentaryKind::Comment,
        "margin note",
    )
    .unwrap();

    parent.retitle("Renamed Parent");
    book.save_note(&parent).unwrap();
    mark_parent_commentary_for_deletion(&book, parent.id.as_str()).unwrap();

    let listed = list_commentary(&book, parent.id.as_str(), false).unwrap();
    assert!(listed.is_empty());
    let resolved = list_commentary(&book, parent.id.as_str(), true).unwrap();
    assert_eq!(resolved.len(), 1);
    assert!(resolved[0]
        .metadata
        .parent_note_id
        .same_identity(&parent.id));
}

#[test]
fn restore_parent_commentary_keeps_resolved_items_resolved() {
    let (_dir, book) = book();
    let parent = note(&book, "body");
    let open =
        create_commentary(&book, parent.id.as_str(), CommentaryKind::Comment, "open").unwrap();
    let pinned =
        create_commentary(&book, parent.id.as_str(), CommentaryKind::Comment, "pinned").unwrap();
    pin_commentary(&book, &pinned.id).unwrap();
    let dismissed = create_commentary(
        &book,
        parent.id.as_str(),
        CommentaryKind::Comment,
        "dismissed",
    )
    .unwrap();
    dismiss_commentary(&book, &dismissed.id).unwrap();

    mark_parent_commentary_for_deletion(&book, parent.id.as_str()).unwrap();
    assert!(list_commentary(&book, parent.id.as_str(), false)
        .unwrap()
        .is_empty());

    restore_parent_commentary_from_deletion(&book, parent.id.as_str()).unwrap();

    let active = list_commentary(&book, parent.id.as_str(), false).unwrap();
    assert_eq!(active.len(), 2);
    assert!(active.iter().any(|item| item.id == open.id));
    assert!(active.iter().any(|item| item.id == pinned.id));

    let resolved = list_commentary(&book, parent.id.as_str(), true).unwrap();
    let dismissed = resolved
        .iter()
        .find(|item| item.id == dismissed.id)
        .expect("dismissed commentary remains stored");
    assert!(dismissed.metadata.status == CommentaryStatus::Dismissed);
    assert!(book
        .read_commentary_note(&NoteId::parse(&dismissed.id).unwrap())
        .unwrap()
        .metadata
        .lifecycle
        .marked_for_deletion_at
        .is_some());
}

#[test]
fn fact_check_gate_applies_only_with_linked_passing_fact_check() {
    let (_dir, book) = book();
    let mut parent = note(&book, "original");
    parent.metadata.lifecycle.lock = LockMode::FactCheckGate;
    book.save_note(&parent).unwrap();
    let proposal = create_commentary(
        &book,
        parent.id.as_str(),
        CommentaryKind::Proposal,
        "checked replacement",
    )
    .unwrap();

    let mut unrelated_meta = CommentaryMetadata::new(
        parent.id.clone(),
        CommentaryKind::FactCheck,
        CommentarySource::Ai,
    );
    unrelated_meta.fact_check_passed = Some(true);
    let mut unrelated = book
        .new_commentary_note("unlinked fact check", unrelated_meta)
        .unwrap();
    unrelated.body = "Looks fine, but not linked.".to_string();
    book.save_commentary_note(&unrelated).unwrap();

    let err = apply_commentary(&book, &proposal.id, ApplyCommentaryOptions::default())
        .expect_err("unlinked fact-check must not satisfy the gate");
    assert!(err.to_string().contains("requires a passing fact-check"));

    let mut linked_meta = CommentaryMetadata::new(
        parent.id.clone(),
        CommentaryKind::FactCheck,
        CommentarySource::Ai,
    );
    linked_meta.fact_check_passed = Some(true);
    linked_meta.approves_commentary_id = Some(NoteId::parse(&proposal.id).unwrap());
    let mut linked = book
        .new_commentary_note("linked fact check", linked_meta)
        .unwrap();
    linked.body = "Passing linked fact-check.".to_string();
    book.save_commentary_note(&linked).unwrap();

    let updated = apply_commentary(&book, &proposal.id, ApplyCommentaryOptions::default()).unwrap();
    assert_eq!(updated.body, "checked replacement");
}
