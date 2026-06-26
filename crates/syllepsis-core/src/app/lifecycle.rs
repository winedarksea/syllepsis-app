//! Application command surface for the **privacy & lifecycle** policy (privacy-security.md):
//! marking content private/archived/locked, the delayed-deletion ("mark for deletion") flow with
//! its scheduled purge, vanishing notes, and the centralized policy overview that backs the
//! settings panel.
//!
//! The on-disk fields these act on ([`Lifecycle`](crate::model::metadata::Lifecycle),
//! [`LockMode`], `Category::private`) shipped in Phase 1; this is the behavior layer that finally
//! reads them. Like the rest of [`crate::app`], every function is a framework-agnostic operation
//! over a [`Book`] the Tauri shell wraps as a command.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::app::dto::NoteDto;
use crate::error::{CoreError, CoreResult};
use crate::id::NoteId;
use crate::model::metadata::LockMode;
use crate::model::{Note, ObjectType};
use crate::storage::{Book, NoteStore};
use crate::sync::AssetRegistry;

/// One note awaiting permanent removal, with the moment its delay elapses so the UI can show a
/// countdown ("purges in 3 days") and offer restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingDeletion {
    pub id: String,
    pub title: String,
    pub marked_at: DateTime<Utc>,
    pub purge_at: DateTime<Utc>,
}

/// A lightweight note reference for the policy lists (id + title only; the panel links out to the
/// full note).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteRef {
    pub id: String,
    pub title: String,
}

impl NoteRef {
    fn of(note: &Note) -> NoteRef {
        NoteRef {
            id: note.id.to_string(),
            title: note.title.clone(),
        }
    }
}

/// A locked note plus its lock mode, for the policy panel's "locked" section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedNote {
    pub id: String,
    pub title: String,
    pub mode: LockMode,
}

/// The single "what is restricted in this book" snapshot the centralized policy view renders
/// (privacy-security.md "Centralized Policy View").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyOverview {
    pub private_notes: Vec<NoteRef>,
    pub archived_notes: Vec<NoteRef>,
    pub locked_notes: Vec<LockedNote>,
    pub pending_deletion: Vec<PendingDeletion>,
    pub private_categories: Vec<String>,
}

/// Load a note, apply `mutate`, stamp `updated`, persist, and return the API shape. The shared
/// spine of every single-note lifecycle toggle below.
fn edit_note(book: &Book, id: &str, mutate: impl FnOnce(&mut Note)) -> CoreResult<NoteDto> {
    let mut note = book.store.read_note(&NoteId::parse(id)?)?;
    mutate(&mut note);
    note.metadata.dates.updated = Utc::now();
    book.save_note(&note)?;
    Ok(NoteDto::from_note(&note))
}

/// Toggle a note's `private` flag. Private notes drop out of default views and RAG retrieval and
/// are excluded from the git publish (see [`crate::app::publish`]).
pub fn set_note_private(book: &Book, id: &str, private: bool) -> CoreResult<NoteDto> {
    edit_note(book, id, |note| note.metadata.lifecycle.private = private)
}

/// Toggle a note's `archived` flag (hidden from default views but kept and reversible).
pub fn set_note_archived(book: &Book, id: &str, archived: bool) -> CoreResult<NoteDto> {
    let note = book.store.read_note(&NoteId::parse(id)?)?;
    if archived && matches!(note.object_type, ObjectType::Picture | ObjectType::Drawing) {
        return Err(CoreError::InvalidBook(
            "pictures and drawings cannot be archived; delete them instead".to_string(),
        ));
    }
    edit_note(book, id, |note| note.metadata.lifecycle.archived = archived)
}

/// Set a note's lock mode (`None`, `UnlockDelay`, or `FactCheckGate`).
pub fn set_note_lock(book: &Book, id: &str, mode: LockMode) -> CoreResult<NoteDto> {
    edit_note(book, id, |note| note.metadata.lifecycle.lock = mode)
}

/// Toggle a category's `private` flag (excluded from publish; its notes drop from default views).
pub fn set_category_private(book: &Book, name: &str, private: bool) -> CoreResult<()> {
    let mut category = book
        .store
        .read_category(name)
        .unwrap_or_else(|_| crate::model::Category::new(name.to_string()));
    category.private = private;
    book.store.write_category(&category)
}

/// "Mark for deletion": start the deletion-delay window rather than removing the note now
/// (privacy-security.md "Deletion Delay"). The note stays on disk but drops out of default views
/// until [`purge_expired`] removes it once the configured delay elapses; [`restore_note`] cancels.
pub fn request_deletion(book: &Book, id: &str) -> CoreResult<NoteDto> {
    let note = book.store.read_note(&NoteId::parse(id)?)?;
    if matches!(note.object_type, ObjectType::Picture | ObjectType::Drawing) {
        return Err(CoreError::InvalidBook(
            "pictures and drawings are deleted immediately after confirmation".to_string(),
        ));
    }
    let updated = edit_note(book, id, |note| {
        note.metadata.lifecycle.marked_for_deletion_at = Some(Utc::now())
    })?;
    crate::app::commentary::mark_parent_commentary_for_deletion(book, id)?;
    Ok(updated)
}

/// Permanently delete a first-class Picture/Drawing note and its tracked asset immediately.
pub fn delete_image_object_now(book: &Book, id: &str) -> CoreResult<()> {
    let id = NoteId::parse(id)?;
    let note = book.store.read_note(&id)?;
    if !matches!(note.object_type, ObjectType::Picture | ObjectType::Drawing) {
        return Err(CoreError::InvalidBook(
            "immediate asset deletion is only for picture and drawing notes".to_string(),
        ));
    }
    if let Some(asset) = &note.asset {
        if let Some(relative_path) = AssetRegistry::scan(&book.root)?.resolve(&asset.uuid) {
            let asset_path = book.root.join(relative_path);
            let sidecar_path = asset_path.with_file_name(format!(
                "{}.uuid",
                asset_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("asset")
            ));
            let _ = std::fs::remove_file(&asset_path);
            let _ = std::fs::remove_file(&sidecar_path);
        }
    }
    book.delete_note(&id)
}

/// Cancel a pending deletion, returning the note to active use.
pub fn restore_note(book: &Book, id: &str) -> CoreResult<NoteDto> {
    edit_note(book, id, |note| {
        note.metadata.lifecycle.marked_for_deletion_at = None
    })
}

/// Permanently remove every note whose deletion delay has elapsed or whose `vanish_at` has passed,
/// as of `now`. This is the scheduled purge the shell runs on startup (and the user can trigger).
/// Returns the ids actually removed. Driven by `now` rather than [`Utc::now`] so it is testable.
pub fn purge_expired(book: &Book, now: DateTime<Utc>) -> CoreResult<Vec<String>> {
    let delay = Duration::days(book.config.cleanup.deletion_delay_days as i64);
    let mut purged = Vec::new();
    for note in book.store.read_all_notes()? {
        if is_due_for_purge(&note, delay, now) {
            book.delete_note(&note.id)?;
            purged.push(note.id.to_string());
        }
    }
    for note in book.read_all_commentary_notes()? {
        if is_due_for_purge(&note, delay, now) {
            book.delete_commentary_note(&note.id)?;
            purged.push(note.id.to_string());
        }
    }
    Ok(purged)
}

/// [`purge_expired`] as of the current wall clock — the entry point the shell calls on startup or
/// from a "empty trash now" action.
pub fn purge_expired_now(book: &Book) -> CoreResult<Vec<String>> {
    purge_expired(book, Utc::now())
}

/// Whether a note has passed either expiry clock: the deletion-delay window after a
/// `marked_for_deletion_at`, or its self-destruct `vanish_at`.
fn is_due_for_purge(note: &Note, delay: Duration, now: DateTime<Utc>) -> bool {
    let deletion_due = note
        .metadata
        .lifecycle
        .marked_for_deletion_at
        .is_some_and(|marked| now >= marked + delay);
    let vanish_due = note
        .metadata
        .lifecycle
        .vanish_at
        .is_some_and(|vanish| now >= vanish);
    deletion_due || vanish_due
}

/// Aggregate every restriction in the book into one [`PolicyOverview`] for the settings panel.
pub fn policy_overview(book: &Book) -> CoreResult<PolicyOverview> {
    let delay = Duration::days(book.config.cleanup.deletion_delay_days as i64);
    let mut overview = PolicyOverview {
        private_notes: Vec::new(),
        archived_notes: Vec::new(),
        locked_notes: Vec::new(),
        pending_deletion: Vec::new(),
        private_categories: Vec::new(),
    };

    for note in book
        .store
        .read_all_notes()?
        .into_iter()
        .chain(book.read_all_commentary_notes()?.into_iter())
    {
        let life = &note.metadata.lifecycle;
        if life.private {
            overview.private_notes.push(NoteRef::of(&note));
        }
        if life.archived {
            overview.archived_notes.push(NoteRef::of(&note));
        }
        if life.lock != LockMode::None {
            overview.locked_notes.push(LockedNote {
                id: note.id.to_string(),
                title: note.title.clone(),
                mode: life.lock,
            });
        }
        if let Some(marked_at) = life.marked_for_deletion_at {
            overview.pending_deletion.push(PendingDeletion {
                id: note.id.to_string(),
                title: note.title.clone(),
                marked_at,
                purge_at: marked_at + delay,
            });
        }
    }

    overview.private_categories = book
        .store
        .categories()?
        .into_iter()
        .filter(|c| c.private)
        .map(|c| c.name)
        .collect();

    // Stable ordering so the panel does not reshuffle between reads.
    overview.private_notes.sort_by(|a, b| a.title.cmp(&b.title));
    overview
        .archived_notes
        .sort_by(|a, b| a.title.cmp(&b.title));
    overview.locked_notes.sort_by(|a, b| a.title.cmp(&b.title));
    overview
        .pending_deletion
        .sort_by(|a, b| a.purge_at.cmp(&b.purge_at));
    overview.private_categories.sort();
    Ok(overview)
}

/// Whether a locked note's protected body may change right now, and why not if it cannot. Pure
/// over its inputs so the policy is unit-testable without a clock or a book.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeGate {
    /// Unlocked, or the lock's condition is satisfied — the edit/merge may proceed.
    Allowed,
    /// `UnlockDelay`: the proposed change must wait until this instant before it can merge.
    WaitUntil(DateTime<Utc>),
    /// `FactCheckGate`: a passing fact-check is required and was not supplied.
    NeedsFactCheck,
}

/// Evaluate a locked note's merge gate. `proposed_at` is when the change was proposed (a
/// proposal's creation time, or the edit time for a direct save); `fact_check_passed` reports
/// whether a passing fact-check accompanies the change. `None` lock ⇒ always [`MergeGate::Allowed`].
pub fn evaluate_merge_gate(
    lock: LockMode,
    proposed_at: DateTime<Utc>,
    fact_check_passed: bool,
    privacy: &crate::config::PrivacyConfig,
    now: DateTime<Utc>,
) -> MergeGate {
    match lock {
        LockMode::None => MergeGate::Allowed,
        LockMode::UnlockDelay => {
            let eligible_at = proposed_at + Duration::hours(privacy.unlock_delay_hours as i64);
            if now >= eligible_at {
                MergeGate::Allowed
            } else {
                MergeGate::WaitUntil(eligible_at)
            }
        }
        LockMode::FactCheckGate => {
            if fact_check_passed {
                MergeGate::Allowed
            } else {
                MergeGate::NeedsFactCheck
            }
        }
    }
}

/// Guard a body-replacing change to a locked note, turning a non-[`MergeGate::Allowed`] gate into a
/// typed [`CoreError::Locked`] the UI can surface (with the unlock time / fact-check requirement).
pub fn guard_locked_merge(
    note: &Note,
    proposed_at: DateTime<Utc>,
    fact_check_passed: bool,
    privacy: &crate::config::PrivacyConfig,
    now: DateTime<Utc>,
) -> CoreResult<()> {
    match evaluate_merge_gate(
        note.metadata.lifecycle.lock,
        proposed_at,
        fact_check_passed,
        privacy,
        now,
    ) {
        MergeGate::Allowed => Ok(()),
        MergeGate::WaitUntil(when) => Err(crate::error::CoreError::Locked(format!(
            "'{}' is locked with an unlock delay; the change can be merged after {}",
            note.title,
            when.to_rfc3339()
        ))),
        MergeGate::NeedsFactCheck => Err(crate::error::CoreError::Locked(format!(
            "'{}' requires a passing fact-check before its body can be rewritten",
            note.title
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::create_note;
    use crate::model::{Category, ObjectType};

    fn book() -> (tempfile::TempDir, Book) {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path(), "Test").unwrap();
        (dir, book)
    }

    #[test]
    fn private_note_drops_out_of_default_listings() {
        let (_d, book) = book();
        let note = create_note(&book, ObjectType::Note, "secret", None).unwrap();
        assert_eq!(crate::app::commands::list_notes(&book).unwrap().len(), 1);

        set_note_private(&book, &note.id, true).unwrap();
        assert!(
            crate::app::commands::list_notes(&book).unwrap().is_empty(),
            "private notes are hidden from default views"
        );
        // Still directly fetchable (the editor can open it).
        assert!(crate::app::commands::get_note(&book, &note.id).is_ok());
    }

    #[test]
    fn deletion_is_delayed_then_purged() {
        let (_d, book) = book();
        let note = create_note(&book, ObjectType::Note, "doomed", None).unwrap();

        // Marking for deletion keeps the file but hides it.
        request_deletion(&book, &note.id).unwrap();
        assert!(crate::app::commands::list_notes(&book).unwrap().is_empty());
        assert!(crate::app::commands::get_note(&book, &note.id).is_ok());

        // A purge run before the delay elapses removes nothing.
        let just_after = Utc::now() + Duration::hours(1);
        assert!(purge_expired(&book, just_after).unwrap().is_empty());

        // A purge run past the configured delay removes it for good.
        let past_delay =
            Utc::now() + Duration::days(book.config.cleanup.deletion_delay_days as i64 + 1);
        let purged = purge_expired(&book, past_delay).unwrap();
        assert_eq!(purged, vec![note.id.clone()]);
        assert!(crate::app::commands::get_note(&book, &note.id).is_err());
    }

    #[test]
    fn restore_cancels_a_pending_deletion() {
        let (_d, book) = book();
        let note = create_note(&book, ObjectType::Note, "saved", None).unwrap();
        request_deletion(&book, &note.id).unwrap();
        restore_note(&book, &note.id).unwrap();

        let past_delay = Utc::now() + Duration::days(3650);
        assert!(purge_expired(&book, past_delay).unwrap().is_empty());
        assert_eq!(crate::app::commands::list_notes(&book).unwrap().len(), 1);
    }

    #[test]
    fn vanishing_note_self_destructs_at_its_time() {
        let (_d, book) = book();
        let mut note = create_note(&book, ObjectType::Note, "ephemeral", None).unwrap();
        let mut stored = book
            .store
            .read_note(&NoteId::parse(&note.id).unwrap())
            .unwrap();
        stored.metadata.lifecycle.vanish_at = Some(Utc::now() + Duration::hours(2));
        book.save_note(&stored).unwrap();
        note.id = stored.id.to_string();

        assert!(purge_expired(&book, Utc::now()).unwrap().is_empty());
        let after = Utc::now() + Duration::hours(3);
        assert_eq!(purge_expired(&book, after).unwrap().len(), 1);
    }

    #[test]
    fn picture_delete_removes_note_asset_and_uuid_sidecar_immediately() {
        let (directory, book) = book();
        let source = directory.path().join("photo.png");
        image::DynamicImage::new_rgb8(3, 2)
            .save_with_format(&source, image::ImageFormat::Png)
            .unwrap();
        let imported =
            crate::app::image_assets::import_image_object(&book, source.to_str().unwrap(), None)
                .unwrap();
        let asset = imported.asset.as_ref().unwrap();
        let relative_path = AssetRegistry::scan(&book.root)
            .unwrap()
            .resolve(&asset.uuid)
            .unwrap()
            .to_string();
        let asset_path = book.root.join(&relative_path);
        let sidecar_path = asset_path.with_file_name(format!(
            "{}.uuid",
            asset_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap()
        ));
        assert!(asset_path.exists());
        assert!(sidecar_path.exists());

        delete_image_object_now(&book, &imported.id).unwrap();

        assert!(crate::app::commands::get_note(&book, &imported.id).is_err());
        assert!(!asset_path.exists());
        assert!(!sidecar_path.exists());
    }

    #[test]
    fn policy_overview_collects_every_restriction() {
        let (_d, book) = book();
        let p = create_note(&book, ObjectType::Note, "private one", None).unwrap();
        set_note_private(&book, &p.id, true).unwrap();
        let l = create_note(&book, ObjectType::Note, "locked one", None).unwrap();
        set_note_lock(&book, &l.id, LockMode::UnlockDelay).unwrap();
        let d = create_note(&book, ObjectType::Note, "trashed one", None).unwrap();
        request_deletion(&book, &d.id).unwrap();

        let mut secret_cat = Category::new("secret");
        secret_cat.private = true;
        book.store.write_category(&secret_cat).unwrap();

        let overview = policy_overview(&book).unwrap();
        assert_eq!(overview.private_notes.len(), 1);
        assert_eq!(overview.locked_notes[0].mode, LockMode::UnlockDelay);
        assert_eq!(overview.pending_deletion.len(), 1);
        assert!(overview.pending_deletion[0].purge_at > overview.pending_deletion[0].marked_at);
        assert_eq!(overview.private_categories, vec!["secret".to_string()]);
    }

    #[test]
    fn category_private_toggle_persists() {
        let (_d, book) = book();
        book.store.write_category(&Category::new("rooms")).unwrap();
        set_category_private(&book, "rooms", true).unwrap();
        assert!(book.store.read_category("rooms").unwrap().private);
    }

    #[test]
    fn unlock_delay_gate_blocks_then_allows_after_the_window() {
        let cfg = crate::config::PrivacyConfig::default(); // 24h
        let proposed = Utc::now();
        // Immediately after proposing: blocked with a future eligibility time.
        match evaluate_merge_gate(LockMode::UnlockDelay, proposed, false, &cfg, proposed) {
            MergeGate::WaitUntil(when) => assert!(when > proposed),
            other => panic!("expected WaitUntil, got {other:?}"),
        }
        // After the delay: allowed.
        let later = proposed + Duration::hours(cfg.unlock_delay_hours as i64 + 1);
        assert_eq!(
            evaluate_merge_gate(LockMode::UnlockDelay, proposed, false, &cfg, later),
            MergeGate::Allowed
        );
    }

    #[test]
    fn fact_check_gate_requires_a_passing_check() {
        let cfg = crate::config::PrivacyConfig::default();
        let now = Utc::now();
        assert_eq!(
            evaluate_merge_gate(LockMode::FactCheckGate, now, false, &cfg, now),
            MergeGate::NeedsFactCheck
        );
        assert_eq!(
            evaluate_merge_gate(LockMode::FactCheckGate, now, true, &cfg, now),
            MergeGate::Allowed
        );
    }

    #[test]
    fn unlocked_note_is_always_allowed() {
        let cfg = crate::config::PrivacyConfig::default();
        let now = Utc::now();
        assert_eq!(
            evaluate_merge_gate(LockMode::None, now, false, &cfg, now),
            MergeGate::Allowed
        );
    }
}
