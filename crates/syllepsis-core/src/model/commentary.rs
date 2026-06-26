//! Typed metadata for commentary notes.
//!
//! Commentary is markdown-backed so it remains inspectable on disk, but it is a child object of a
//! parent note rather than part of the first-class note corpus. These fields give the app enough
//! structure to drive review, locking, and AI proposal workflows without parsing body markers.

use serde::{Deserialize, Serialize};

use crate::id::NoteId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentaryKind {
    Proposal,
    FactCheck,
    Critique,
    Comment,
    Footnote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentaryStatus {
    Locked,
    Open,
    Merged,
    Dismissed,
    Pinned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentarySource {
    Ai,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentaryTargetField {
    Body,
    Summary,
    Categories,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentaryMetadata {
    pub parent_note_id: NoteId,
    pub kind: CommentaryKind,
    pub status: CommentaryStatus,
    pub source: CommentarySource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_field: Option<CommentaryTargetField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_body_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crdt_backend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_crdt_snapshot_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact_check_passed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approves_commentary_id: Option<NoteId>,
}

impl CommentaryMetadata {
    pub fn new(
        parent_note_id: NoteId,
        kind: CommentaryKind,
        source: CommentarySource,
    ) -> CommentaryMetadata {
        CommentaryMetadata {
            parent_note_id,
            kind,
            status: CommentaryStatus::Open,
            source,
            target_field: None,
            job_id: None,
            task: None,
            provider: None,
            model: None,
            base_body_sha256: None,
            base_body: None,
            crdt_backend: None,
            base_crdt_snapshot_b64: None,
            fact_check_passed: None,
            approves_commentary_id: None,
        }
    }
}
