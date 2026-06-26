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
