//! Commentary child-object behavior.
//!
//! Commentary lives in `_commentary/` and is linked to a parent note by typed frontmatter. It is
//! used for AI results, user comments, locked-note edit drafts, and pinned footnotes.

use base64::Engine as _;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::app::dto::NoteDto;
use crate::error::{CoreError, CoreResult};
use crate::id::NoteId;
use crate::llm::{parse_category_list, parse_fact_check_response, LlmTask, Proposal};
use crate::model::{
    CommentaryKind, CommentaryMetadata, CommentarySource, CommentaryStatus, CommentaryTargetField,
    FactCheckAssessment, LockMode, Note, ObjectType,
};
use crate::storage::{layout, Book, NoteStore};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ApplyCommentaryOptions {
    /// Permit a stale body proposal to replace the current body when CRDT merge is unavailable.
    pub force_replace: bool,
    /// Explicit user/tool confirmation that a fact-check gate has passed.
    pub fact_check_passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentarySummary {
    pub id: String,
    pub title: String,
    pub body: String,
    pub metadata: CommentaryMetadata,
    pub created: chrono::DateTime<Utc>,
    pub updated: chrono::DateTime<Utc>,
}

pub fn list_commentary(
    book: &Book,
    parent_note_id: &str,
    include_resolved: bool,
) -> CoreResult<Vec<CommentarySummary>> {
    let parent = NoteId::parse(parent_note_id)?;
    let mut commentary = book
        .read_all_commentary_notes()?
        .into_iter()
        .filter_map(|note| summary_if_matches_parent(note, &parent, include_resolved))
        .collect::<Vec<_>>();
    commentary.sort_by(|a, b| b.created.cmp(&a.created));
    Ok(commentary)
}

pub fn get_commentary(book: &Book, commentary_id: &str) -> CoreResult<NoteDto> {
    let note = book.read_commentary_note(&NoteId::parse(commentary_id)?)?;
    Ok(NoteDto::from_note(&note))
}

pub fn create_commentary(
    book: &Book,
    parent_note_id: &str,
    kind: CommentaryKind,
    body: &str,
) -> CoreResult<NoteDto> {
    let parent = book.store.read_note(&NoteId::parse(parent_note_id)?)?;
    let mut metadata = CommentaryMetadata::new(parent.id.clone(), kind, CommentarySource::User);
    if kind == CommentaryKind::Proposal {
        metadata.target_field = Some(CommentaryTargetField::Body);
        attach_merge_base(book, &parent, &mut metadata)?;
        if parent.metadata.lifecycle.lock == LockMode::UnlockDelay {
            metadata.status = CommentaryStatus::Locked;
        }
    }
    let title = format!("{} on {}", label_for_kind(kind), parent.title);
    let mut note = book.new_commentary_note(title, metadata)?;
    note.body = body.to_string();
    book.save_commentary_note(&note)?;
    Ok(NoteDto::from_note(&note))
}

pub fn update_commentary(book: &Book, commentary: NoteDto) -> CoreResult<NoteDto> {
    let mut note = commentary.into_note(book.config.markdown.dialect_version.clone())?;
    if note.object_type != ObjectType::Commentary {
        return Err(CoreError::InvalidBook(
            "update_commentary only accepts commentary objects".to_string(),
        ));
    }
    note.metadata.dates.updated = Utc::now();
    book.save_commentary_note(&note)?;
    Ok(NoteDto::from_note(&note))
}

pub fn apply_commentary(
    book: &Book,
    commentary_id: &str,
    options: ApplyCommentaryOptions,
) -> CoreResult<NoteDto> {
    let commentary_id = NoteId::parse(commentary_id)?;
    let mut commentary = book.read_commentary_note(&commentary_id)?;
    let meta = commentary_metadata_ref(&commentary)?.clone();
    let mut parent = book.store.read_note(&meta.parent_note_id)?;

    match meta.kind {
        CommentaryKind::Proposal => {
            apply_proposal_commentary(book, &mut parent, &commentary, &meta, &options)?
        }
        CommentaryKind::FactCheck
        | CommentaryKind::Critique
        | CommentaryKind::Comment
        | CommentaryKind::Footnote => {}
    }

    commentary
        .commentary
        .as_mut()
        .expect("checked above")
        .status = CommentaryStatus::Merged;
    commentary.metadata.lifecycle.marked_for_deletion_at = Some(Utc::now());
    commentary.metadata.dates.updated = Utc::now();
    book.save_commentary_note(&commentary)?;
    Ok(NoteDto::from_note(&parent))
}

pub fn dismiss_commentary(book: &Book, commentary_id: &str) -> CoreResult<NoteDto> {
    let mut commentary = book.read_commentary_note(&NoteId::parse(commentary_id)?)?;
    commentary_metadata_ref(&commentary)?;
    commentary
        .commentary
        .as_mut()
        .expect("checked above")
        .status = CommentaryStatus::Dismissed;
    commentary.metadata.lifecycle.marked_for_deletion_at = Some(Utc::now());
    commentary.metadata.dates.updated = Utc::now();
    book.save_commentary_note(&commentary)?;
    Ok(NoteDto::from_note(&commentary))
}

pub fn pin_commentary(book: &Book, commentary_id: &str) -> CoreResult<NoteDto> {
    let mut commentary = book.read_commentary_note(&NoteId::parse(commentary_id)?)?;
    let meta = commentary_metadata(&mut commentary)?;
    meta.status = CommentaryStatus::Pinned;
    meta.kind = CommentaryKind::Footnote;
    commentary.metadata.lifecycle.marked_for_deletion_at = None;
    commentary.metadata.dates.updated = Utc::now();
    book.save_commentary_note(&commentary)?;
    Ok(NoteDto::from_note(&commentary))
}

pub fn create_proposal_commentary(
    book: &Book,
    proposal: &Proposal,
    job_id: Option<&str>,
) -> CoreResult<NoteDto> {
    let parent = book.store.read_note(&proposal.target)?;
    let mut metadata = CommentaryMetadata::new(
        parent.id.clone(),
        kind_for_task(proposal.task),
        CommentarySource::Ai,
    );
    metadata.target_field = Some(target_field_for_task(proposal.task));
    metadata.job_id = job_id.map(str::to_string);
    metadata.task = Some(proposal.task.as_str().to_string());
    metadata.provider = Some(proposal.provider.clone());
    metadata.model = Some(proposal.model.clone());
    if proposal.task.replaces_body() {
        attach_merge_base(book, &parent, &mut metadata)?;
        if parent.metadata.lifecycle.lock == LockMode::UnlockDelay {
            metadata.status = CommentaryStatus::Locked;
        }
    }
    let commentary_body = if proposal.task == LlmTask::FactCheck {
        let (assessment, notes) = parse_fact_check_response(&proposal.content);
        metadata.fact_check_assessment = Some(assessment);
        metadata.fact_check_passed = Some(matches!(
            assessment,
            FactCheckAssessment::StrongEvidence
                | FactCheckAssessment::SomeQuestionablePoints
                | FactCheckAssessment::NoCheckableClaims
        ));
        notes
    } else {
        proposal.content.clone()
    };

    let title = format!("{} proposal for {}", proposal.task.as_str(), parent.title);
    let mut commentary = book.new_commentary_note(title, metadata)?;
    commentary.body = commentary_body;
    commentary.metadata.authorship.ai_generated = true;
    book.save_commentary_note(&commentary)?;
    Ok(NoteDto::from_note(&commentary))
}

pub fn create_previous_version_commentary(
    book: &Book,
    parent: &Note,
    body: &str,
) -> CoreResult<()> {
    let mut metadata = CommentaryMetadata::new(
        parent.id.clone(),
        CommentaryKind::Footnote,
        CommentarySource::Ai,
    );
    metadata.status = CommentaryStatus::Pinned;
    metadata.target_field = Some(CommentaryTargetField::Body);
    let mut commentary =
        book.new_commentary_note(format!("Previous version of {}", parent.title), metadata)?;
    commentary.body = body.to_string();
    commentary.metadata.authorship.ai_generated = true;
    book.save_commentary_note(&commentary)?;
    Ok(())
}

pub fn mark_parent_commentary_for_deletion(book: &Book, parent_note_id: &str) -> CoreResult<()> {
    let parent = NoteId::parse(parent_note_id)?;
    for mut commentary in book.read_all_commentary_notes()? {
        if commentary
            .commentary
            .as_ref()
            .is_some_and(|meta| meta.parent_note_id == parent)
        {
            commentary.metadata.lifecycle.marked_for_deletion_at = Some(Utc::now());
            commentary.metadata.dates.updated = Utc::now();
            book.save_commentary_note(&commentary)?;
        }
    }
    Ok(())
}

pub fn delete_parent_commentary_now(book: &Book, parent_note_id: &str) -> CoreResult<()> {
    let parent = NoteId::parse(parent_note_id)?;
    for commentary in book.read_all_commentary_notes()? {
        if commentary
            .commentary
            .as_ref()
            .is_some_and(|meta| meta.parent_note_id == parent)
        {
            book.delete_commentary_note(&commentary.id)?;
        }
    }
    Ok(())
}

fn apply_proposal_commentary(
    book: &Book,
    parent: &mut Note,
    commentary: &Note,
    meta: &CommentaryMetadata,
    options: &ApplyCommentaryOptions,
) -> CoreResult<()> {
    if meta.status == CommentaryStatus::Locked {
        crate::app::lifecycle::guard_locked_merge(
            parent,
            commentary.metadata.dates.created,
            options.fact_check_passed,
            &book.config.privacy,
            Utc::now(),
        )?;
    }

    let fact_check_passed =
        options.fact_check_passed || fact_check_passes_for(book, &commentary.id)?;
    if parent.metadata.lifecycle.lock == LockMode::FactCheckGate && !fact_check_passed {
        return Err(CoreError::Locked(format!(
            "'{}' requires a passing fact-check before this commentary can be applied",
            parent.title
        )));
    }

    match meta.target_field.unwrap_or(CommentaryTargetField::Body) {
        CommentaryTargetField::Body => {
            parent.body = body_after_apply(book, parent, commentary, meta, options)?;
            parent.metadata.authorship.ai_generated |= meta.source == CommentarySource::Ai;
        }
        CommentaryTargetField::Summary => {
            parent.summary = commentary.body.clone();
        }
        CommentaryTargetField::Categories => {
            for tag in parse_category_list(&commentary.body) {
                if !parent.categories.contains(&tag) {
                    parent.categories.push(tag);
                }
            }
        }
    }
    parent.metadata.dates.updated = Utc::now();
    book.save_note(parent)?;
    Ok(())
}

fn body_after_apply(
    book: &Book,
    parent: &Note,
    commentary: &Note,
    meta: &CommentaryMetadata,
    options: &ApplyCommentaryOptions,
) -> CoreResult<String> {
    let current_hash = sha256_hex(&parent.body);
    if meta.base_body_sha256.as_deref() == Some(current_hash.as_str()) {
        return Ok(commentary.body.clone());
    }
    if let Some(merged) = try_crdt_merge_body(book, parent, &commentary.body, meta)? {
        return Ok(merged);
    }
    if options.force_replace {
        return Ok(commentary.body.clone());
    }
    Err(CoreError::InvalidBook(
        "commentary proposal is stale; review the diff before replacing the current body"
            .to_string(),
    ))
}

fn try_crdt_merge_body(
    book: &Book,
    parent: &Note,
    proposal_body: &str,
    meta: &CommentaryMetadata,
) -> CoreResult<Option<String>> {
    if meta.crdt_backend.as_deref() != Some(crate::crdt::LORO_BACKEND) {
        return Ok(None);
    }
    let Some(base_snapshot) = meta.base_crdt_snapshot_b64.as_deref() else {
        return Ok(None);
    };
    let backend = crate::crdt::select_crdt_backend(&book.config.sync);
    if backend.name() != crate::crdt::LORO_BACKEND {
        return Ok(None);
    }
    let actor = crate::sync::actor_id_for(&book.root)?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(base_snapshot)
        .map_err(|error| {
            CoreError::InvalidBook(format!("decode commentary base snapshot: {error}"))
        })?;
    let mut proposal_doc = backend.load_document(&actor, &decoded)?;
    proposal_doc.set_text(proposal_body);

    let sidecar = layout::crdt_sidecar_path(&book.root, &parent.id);
    let mut current_doc = if sidecar.exists() {
        backend.load_document(&actor, &std::fs::read(sidecar)?)?
    } else {
        backend.new_document(&actor, &parent.body)
    };
    current_doc.merge(&proposal_doc.snapshot()?)?;
    Ok(Some(current_doc.text()))
}

fn attach_merge_base(
    book: &Book,
    parent: &Note,
    metadata: &mut CommentaryMetadata,
) -> CoreResult<()> {
    metadata.base_body_sha256 = Some(sha256_hex(&parent.body));
    metadata.base_body = Some(parent.body.clone());
    let backend = crate::crdt::select_crdt_backend(&book.config.sync);
    let actor = crate::sync::actor_id_for(&book.root)?;
    let sidecar = layout::crdt_sidecar_path(&book.root, &parent.id);
    let doc = if sidecar.exists() {
        backend.load_document(&actor, &std::fs::read(sidecar)?)?
    } else {
        backend.new_document(&actor, &parent.body)
    };
    metadata.crdt_backend = Some(backend.name().to_string());
    metadata.base_crdt_snapshot_b64 =
        Some(base64::engine::general_purpose::STANDARD.encode(doc.snapshot()?));
    Ok(())
}

fn fact_check_passes_for(book: &Book, commentary_id: &NoteId) -> CoreResult<bool> {
    Ok(book.read_all_commentary_notes()?.into_iter().any(|note| {
        note.commentary.as_ref().is_some_and(|meta| {
            meta.kind == CommentaryKind::FactCheck
                && meta.status != CommentaryStatus::Dismissed
                && meta.fact_check_passed == Some(true)
                && meta.approves_commentary_id.as_ref() == Some(commentary_id)
        })
    }))
}

fn summary_if_matches_parent(
    note: Note,
    parent: &NoteId,
    include_resolved: bool,
) -> Option<CommentarySummary> {
    let metadata = note.commentary.clone()?;
    if &metadata.parent_note_id != parent {
        return None;
    }
    if !include_resolved
        && matches!(
            metadata.status,
            CommentaryStatus::Merged | CommentaryStatus::Dismissed
        )
    {
        return None;
    }
    if !include_resolved && note.metadata.lifecycle.marked_for_deletion_at.is_some() {
        return None;
    }
    Some(CommentarySummary {
        id: note.id.to_string(),
        title: note.title,
        body: note.body,
        metadata,
        created: note.metadata.dates.created,
        updated: note.metadata.dates.updated,
    })
}

fn commentary_metadata(note: &mut Note) -> CoreResult<&mut CommentaryMetadata> {
    note.commentary.as_mut().ok_or_else(|| {
        CoreError::InvalidBook("commentary note is missing commentary metadata".to_string())
    })
}

fn commentary_metadata_ref(note: &Note) -> CoreResult<&CommentaryMetadata> {
    note.commentary.as_ref().ok_or_else(|| {
        CoreError::InvalidBook("commentary note is missing commentary metadata".to_string())
    })
}

fn kind_for_task(task: LlmTask) -> CommentaryKind {
    match task {
        LlmTask::FactCheck => CommentaryKind::FactCheck,
        LlmTask::DevilsAdvocate => CommentaryKind::Critique,
        _ => CommentaryKind::Proposal,
    }
}

fn target_field_for_task(task: LlmTask) -> CommentaryTargetField {
    match task {
        LlmTask::Summarize => CommentaryTargetField::Summary,
        LlmTask::CategorySuggest => CommentaryTargetField::Categories,
        _ => CommentaryTargetField::Body,
    }
}

fn label_for_kind(kind: CommentaryKind) -> &'static str {
    match kind {
        CommentaryKind::Proposal => "Proposal",
        CommentaryKind::FactCheck => "Fact check",
        CommentaryKind::Critique => "Critique",
        CommentaryKind::Comment => "Comment",
        CommentaryKind::Footnote => "Footnote",
    }
}

fn sha256_hex(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

#[cfg(test)]
mod tests;
