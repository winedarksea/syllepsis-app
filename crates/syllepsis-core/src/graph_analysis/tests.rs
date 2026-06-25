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
                ..Default::default()
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
    assert!(request_json.contains("\"timeline_primary_date\":\"created\""));

    let result = empty_result(GraphMode::Density, "hashing-bow");
    let result_json = serde_json::to_string(&result).unwrap();
    assert!(result_json.contains("\"semantic\":false"));
    // `timeline` is None for non-timeline modes and skipped.
    assert!(!result_json.contains("\"timeline\""));
}

fn add_note_created(book: &Book, title: &str, created: chrono::DateTime<chrono::Utc>) -> String {
    let mut note = book.new_note(ObjectType::Note, title).unwrap();
    note.categories = vec!["log".into()];
    note.metadata.dates.created = created;
    note.metadata.dates.updated = created;
    book.save_note(&note).unwrap();
    note.id.to_string()
}

#[test]
fn timeline_positions_notes_in_chronological_order() {
    use chrono::TimeZone;
    let (_directory, book) = test_book();
    add_note_created(
        &book,
        "Jan",
        chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
    );
    add_note_created(
        &book,
        "Jun",
        chrono::Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap(),
    );
    add_note_created(
        &book,
        "Dec",
        chrono::Utc.with_ymd_and_hms(2024, 12, 1, 0, 0, 0).unwrap(),
    );

    let corpus = SemanticGraphCorpus::build(&book).unwrap();
    let result = corpus
        .analyze(&GraphAnalysisRequest {
            mode: GraphMode::Timeline,
            ..Default::default()
        })
        .unwrap();

    assert_eq!(result.nodes.len(), 3);
    assert!(result.semantic_edges.is_empty());
    let meta = result.timeline.expect("timeline meta present");
    assert_eq!(meta.undated_count, 0);
    assert!(!meta.ticks.is_empty());
    assert!(meta.focus_start_x.is_finite() && meta.focus_end_x.is_finite());
    assert!(result
        .nodes
        .iter()
        .all(|node| (0.0..=1.0).contains(&node.x) && node.x.is_finite() && node.y.is_finite()));
    let x_of = |title: &str| result.nodes.iter().find(|n| n.title == title).unwrap().x;
    assert!(x_of("Jan") < x_of("Jun"));
    assert!(x_of("Jun") < x_of("Dec"));
}

#[test]
fn timeline_falls_back_and_parks_undated_notes() {
    use chrono::{NaiveDate, TimeZone};
    let (_directory, book) = test_book();

    let mut done = book.new_note(ObjectType::Note, "Done").unwrap();
    done.metadata.dates.completed = Some(crate::model::FlexDate {
        date: Some(NaiveDate::from_ymd_opt(2024, 5, 10).unwrap()),
        ..Default::default()
    });
    book.save_note(&done).unwrap();

    let mut open = book.new_note(ObjectType::Note, "Open").unwrap();
    open.metadata.dates.created = chrono::Utc.with_ymd_and_hms(2024, 5, 1, 0, 0, 0).unwrap();
    book.save_note(&open).unwrap();

    let corpus = SemanticGraphCorpus::build(&book).unwrap();

    // With a Created fallback both notes are placed.
    let with_fallback = corpus
        .analyze(&GraphAnalysisRequest {
            mode: GraphMode::Timeline,
            timeline_primary_date: TimelineDateField::Completed,
            timeline_fallback_date: Some(TimelineDateField::Created),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(with_fallback.timeline.unwrap().undated_count, 0);

    // Without a fallback the note lacking a completed date is undated.
    let no_fallback = corpus
        .analyze(&GraphAnalysisRequest {
            mode: GraphMode::Timeline,
            timeline_primary_date: TimelineDateField::Completed,
            timeline_fallback_date: None,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(no_fallback.timeline.unwrap().undated_count, 1);
    let open_node = no_fallback
        .nodes
        .iter()
        .find(|n| n.title == "Open")
        .unwrap();
    assert!(open_node.no_semantic_signal);
}

#[test]
fn timeline_granularity_autoselects_from_span() {
    assert_eq!(
        resolve_granularity(TimelineGranularity::Auto, 0, MS_PER_HOUR),
        TimelineGranularity::Hour
    );
    assert_eq!(
        resolve_granularity(TimelineGranularity::Auto, 0, 10 * MS_PER_DAY),
        TimelineGranularity::Day
    );
    assert_eq!(
        resolve_granularity(TimelineGranularity::Auto, 0, 200 * MS_PER_DAY),
        TimelineGranularity::Month
    );
    assert_eq!(
        resolve_granularity(TimelineGranularity::Auto, 0, 4000 * MS_PER_DAY),
        TimelineGranularity::Year
    );
}

#[test]
fn timeline_buckets_are_calendar_aware() {
    use chrono::TimeZone;
    let ms = |y, m, d| {
        chrono::Utc
            .with_ymd_and_hms(y, m, d, 0, 0, 0)
            .unwrap()
            .timestamp_millis()
    };
    let mid_march = chrono::Utc
        .with_ymd_and_hms(2024, 3, 15, 12, 30, 0)
        .unwrap()
        .timestamp_millis();
    assert_eq!(
        floor_to_bucket(mid_march, TimelineGranularity::Month),
        ms(2024, 3, 1)
    );
    assert_eq!(
        next_bucket(ms(2024, 3, 1), TimelineGranularity::Month),
        ms(2024, 4, 1)
    );
    assert_eq!(
        next_bucket(ms(2024, 12, 1), TimelineGranularity::Month),
        ms(2025, 1, 1)
    );
    assert_eq!(
        floor_to_bucket(ms(2024, 12, 5), TimelineGranularity::Year),
        ms(2024, 1, 1)
    );
    assert_eq!(
        next_bucket(ms(2024, 1, 1), TimelineGranularity::Year),
        ms(2025, 1, 1)
    );
}

#[test]
fn timeline_with_no_dated_notes_parks_everyone() {
    let (_directory, book) = test_book();
    let mut note = book.new_note(ObjectType::Note, "Floating").unwrap();
    note.body = "no resolvable date".into();
    book.save_note(&note).unwrap();

    let corpus = SemanticGraphCorpus::build(&book).unwrap();
    let result = corpus
        .analyze(&GraphAnalysisRequest {
            mode: GraphMode::Timeline,
            timeline_primary_date: TimelineDateField::Scheduled,
            timeline_fallback_date: None,
            ..Default::default()
        })
        .unwrap();
    let meta = result.timeline.unwrap();
    assert_eq!(meta.undated_count, 1);
    assert!(meta.ticks.is_empty());
    assert!(result
        .nodes
        .iter()
        .all(|node| node.x.is_finite() && node.y.is_finite()));
}
