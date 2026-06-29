//! [`SearchFilter`] — structured predicate sent from the UI to narrow search results.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::{ClassificationKind, NoteVisibility, ObjectType};

/// Structured search filter. All fields are optional/defaulted so an empty `SearchFilter` (i.e.
/// `SearchFilter::default()`) reproduces the existing unfiltered behavior — zero categories means
/// any category passes; zero object_types/classifications means any type/classification passes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchFilter {
    /// Which lifecycle bucket to search. Active is the default/RAG-safe corpus.
    pub visibility: NoteVisibility,
    /// Keep only notes in at least one of these categories (empty = any category).
    pub categories: Vec<String>,
    /// Keep only notes updated at or after this timestamp.
    pub updated_after: Option<DateTime<Utc>>,
    /// Keep only notes whose body is at least this many characters.
    pub min_body_len: Option<usize>,
    /// Keep only notes whose body is at most this many characters.
    pub max_body_len: Option<usize>,
    /// Keep only notes of these object types (empty = all types).
    pub object_types: Vec<ObjectType>,
    /// Keep only notes of these classifications (empty = all classifications).
    pub classifications: Vec<ClassificationKind>,
    /// When true, only starred notes pass.
    pub starred_only: bool,
}
