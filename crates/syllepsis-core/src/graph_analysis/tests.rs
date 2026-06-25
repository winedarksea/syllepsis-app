use super::*;
use crate::model::ObjectType;
use crate::storage::Book;

fn test_book() -> (tempfile::TempDir, Book) {
    let directory = tempfile::tempdir().unwrap();
    let book = Book::create(directory.path(), "Graph analysis").unwrap();
    (directory, book)
}

fn add_note(book: &Book, title: &str, body: &str, category: &str) -> String {
    let mut note = book.new_note(ObjectType::Note, title).unwrap();
    note.body = body.into();
    note.categories = vec![category.into()];
    book.save_note(&note).unwrap();
    note.id.to_string()
}

#[test]
fn hidden_notes_are_excluded_from_the_corpus() {
    let (_directory, book) = test_book();
    add_note(&book, "Visible", "garden compost soil", "garden");
    let mut hidden = book.new_note(ObjectType::Note, "Hidden").unwrap();
    hidden.body = "private electrical breaker".into();
    hidden.metadata.lifecycle.private = true;
    book.save_note(&hidden).unwrap();

    let corpus = SemanticGraphCorpus::build(&book).unwrap();
    let result = corpus.analyze(&GraphAnalysisRequest::default()).unwrap();

    assert_eq!(result.nodes.len(), 1);
    assert_eq!(result.nodes[0].title, "Visible");
}

#[test]
fn corpus_fingerprint_changes_when_note_content_changes() {
    let (_directory, book) = test_book();
    let id = add_note(&book, "Draft", "first body", "writing");
    let first = current_corpus_fingerprint(&book).unwrap();
    let note_id = crate::id::NoteId::parse(&id).unwrap();
    let mut note = book.store.read_note(&note_id).unwrap();
    note.body = "second body".into();
    book.save_note(&note).unwrap();
    let second = current_corpus_fingerprint(&book).unwrap();
    assert_ne!(first, second);
}

#[test]
fn corpus_fingerprint_changes_with_relationship_and_embedding_configuration() {
    let (_directory, mut book) = test_book();
    let first_id = add_note(&book, "First", "alpha beta", "one");
    let second_id = add_note(&book, "Second", "beta gamma", "two");
    let initial = current_corpus_fingerprint(&book).unwrap();

    let mut second = book
        .store
        .read_note(&crate::id::NoteId::parse(&second_id).unwrap())
        .unwrap();
    second.prior = Some(crate::model::PriorEdge::follows(
        crate::id::NoteId::parse(&first_id).unwrap(),
        crate::model::PriorKind::NewParagraph,
    ));
    book.save_note(&second).unwrap();
    let with_relationship = current_corpus_fingerprint(&book).unwrap();
    assert_ne!(initial, with_relationship);

    book.config.embedding.dimensions += 1;
    let with_embedding_change = current_corpus_fingerprint(&book).unwrap();
    assert_ne!(with_relationship, with_embedding_change);
}

#[test]
fn every_mode_returns_nodes_edges_and_finite_coordinates() {
    let (_directory, book) = test_book();
    for index in 0..8 {
        let (body, category) = if index < 4 {
            ("garden soil compost flowers", "garden")
        } else {
            ("electrical breaker circuit wiring", "electrical")
        };
        add_note(&book, &format!("Note {index}"), body, category);
    }
    let notes = book.store.read_all_notes().unwrap();
    crate::embeddings::repository::write_test_sidecars(&book, &notes);
    let corpus = SemanticGraphCorpus::build(&book).unwrap();

    for mode in [
        GraphMode::Categories,
        GraphMode::Pillars,
        GraphMode::Communities,
        GraphMode::Density,
    ] {
        let result = corpus
            .analyze(&GraphAnalysisRequest {
                mode,
                umap_neighbors: 3,
                kmeans_k: 2,
                louvain_resolution: 1.0,
                hdbscan_min_cluster_size: 2,
            })
            .unwrap();
        assert_eq!(result.nodes.len(), 8);
        assert!(result
            .nodes
            .iter()
            .all(|node| node.x.is_finite() && node.y.is_finite()));
        assert!(!result.semantic_edges.is_empty());
    }
}

#[test]
fn empty_notes_are_marked_as_having_no_semantic_signal() {
    let (_directory, book) = test_book();
    book.new_note(ObjectType::Note, "").unwrap();
    let corpus = SemanticGraphCorpus::build(&book).unwrap();
    let result = corpus.analyze(&GraphAnalysisRequest::default()).unwrap();
    assert_eq!(result.summary.no_signal_count, 1);
    assert!(result.nodes[0].no_semantic_signal);
}

#[test]
fn request_and_result_shapes_serialize() {
    let request_json = serde_json::to_string(&GraphAnalysisRequest::default()).unwrap();
    assert!(request_json.contains("\"mode\":\"categories\""));

    let result = empty_result(GraphMode::Density, "hashing-bow");
    let result_json = serde_json::to_string(&result).unwrap();
    assert!(result_json.contains("\"semantic\":false"));
}
