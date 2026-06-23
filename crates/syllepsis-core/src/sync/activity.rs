//! Device-local sync activity log for UI observability.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

use crate::error::CoreResult;
use crate::id::NoteId;
use crate::storage::layout;

const ACTIVITY_FILE: &str = "activity.jsonl";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncActivityEvent {
    pub happened_at: DateTime<Utc>,
    pub source: String,
    pub kind: String,
    pub path: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncActivitySummary {
    pub external_updates_24h: usize,
    pub external_updates_7d: usize,
    pub external_note_updates_24h: usize,
    pub latest_external_update_at: Option<DateTime<Utc>>,
    pub conflict_copies_7d: usize,
    pub latest_conflict_path: Option<String>,
    pub latest_conflict_at: Option<DateTime<Utc>>,
    pub remote_loro_merges_7d: usize,
    pub latest_remote_loro_merge_note: Option<String>,
    pub latest_remote_loro_merge_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteSyncActivity {
    pub kind: String,
    pub happened_at: DateTime<Utc>,
    pub detail: String,
}

impl SyncActivityEvent {
    pub fn new(
        source: impl Into<String>,
        kind: impl Into<String>,
        path: Option<String>,
        detail: impl Into<String>,
    ) -> SyncActivityEvent {
        SyncActivityEvent {
            happened_at: Utc::now(),
            source: source.into(),
            kind: kind.into(),
            path,
            detail: detail.into(),
        }
    }
}

pub fn summarize_activity(events: &[SyncActivityEvent], now: DateTime<Utc>) -> SyncActivitySummary {
    let day_cutoff = now - Duration::hours(24);
    let week_cutoff = now - Duration::days(7);
    let mut external_paths_24h = BTreeSet::new();
    let mut external_paths_7d = BTreeSet::new();
    let mut external_notes_24h = BTreeSet::new();
    let mut latest_external_update_at = None;
    let mut conflict_copies_7d = 0;
    let mut latest_conflict_path = None;
    let mut latest_conflict_at = None;
    let mut remote_loro_merges_7d = 0;
    let mut latest_remote_loro_merge_note = None;
    let mut latest_remote_loro_merge_at = None;

    for event in events {
        if event.kind == "external_update" {
            if let Some(path) = event.path.as_deref() {
                if event.happened_at >= day_cutoff {
                    external_paths_24h.insert(path.to_string());
                    if let Some(note_id) = note_id_from_activity_path(path) {
                        external_notes_24h.insert(note_id.as_str().to_string());
                    }
                }
                if event.happened_at >= week_cutoff {
                    external_paths_7d.insert(path.to_string());
                }
            }
            latest_external_update_at = latest_time(latest_external_update_at, event.happened_at);
        } else if event.kind == "conflict_detected" && event.happened_at >= week_cutoff {
            conflict_copies_7d += 1;
            if latest_conflict_at
                .map(|current| event.happened_at > current)
                .unwrap_or(true)
            {
                latest_conflict_at = Some(event.happened_at);
                latest_conflict_path = event.path.clone();
            }
        } else if event.kind == "remote_loro_merge" && event.happened_at >= week_cutoff {
            remote_loro_merges_7d += 1;
            if latest_remote_loro_merge_at
                .map(|current| event.happened_at > current)
                .unwrap_or(true)
            {
                latest_remote_loro_merge_at = Some(event.happened_at);
                latest_remote_loro_merge_note = event.path.clone();
            }
        }
    }

    SyncActivitySummary {
        external_updates_24h: external_paths_24h.len(),
        external_updates_7d: external_paths_7d.len(),
        external_note_updates_24h: external_notes_24h.len(),
        latest_external_update_at,
        conflict_copies_7d,
        latest_conflict_path,
        latest_conflict_at,
        remote_loro_merges_7d,
        latest_remote_loro_merge_note,
        latest_remote_loro_merge_at,
    }
}

pub fn latest_note_activity(
    events: &[SyncActivityEvent],
    note_id: &NoteId,
    now: DateTime<Utc>,
) -> Option<NoteSyncActivity> {
    let cutoff = now - Duration::days(7);
    events
        .iter()
        .filter(|event| event.happened_at >= cutoff)
        .filter(|event| {
            matches!(
                event.kind.as_str(),
                "external_update" | "remote_loro_merge" | "conflict_detected"
            )
        })
        .filter(|event| {
            event
                .path
                .as_deref()
                .and_then(note_id_from_activity_path)
                .as_ref()
                == Some(note_id)
        })
        .max_by(|a, b| a.happened_at.cmp(&b.happened_at))
        .map(|event| NoteSyncActivity {
            kind: event.kind.clone(),
            happened_at: event.happened_at,
            detail: event.detail.clone(),
        })
}

pub fn append_activity(book_root: &Path, event: &SyncActivityEvent) -> CoreResult<()> {
    std::fs::create_dir_all(layout::sync_dir(book_root))?;
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(activity_path(book_root))?;
    writeln!(file, "{}", serde_json::to_string(event)?)?;
    Ok(())
}

pub fn list_activity(book_root: &Path) -> CoreResult<Vec<SyncActivityEvent>> {
    let path = activity_path(book_root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut events = Vec::new();
    for line in std::fs::read_to_string(path)?.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<SyncActivityEvent>(line) {
            events.push(event);
        }
    }
    events.sort_by(|a, b| b.happened_at.cmp(&a.happened_at));
    Ok(events)
}

pub fn prune_activity(book_root: &Path, retention_days: i64) -> CoreResult<()> {
    let cutoff = Utc::now() - Duration::days(retention_days.max(0));
    let mut events = list_activity(book_root)?;
    events.retain(|event| event.happened_at >= cutoff);
    events.sort_by(|a, b| a.happened_at.cmp(&b.happened_at));
    std::fs::create_dir_all(layout::sync_dir(book_root))?;
    let body = events
        .into_iter()
        .map(|event| serde_json::to_string(&event))
        .collect::<Result<Vec<_>, _>>()?
        .join("\n");
    std::fs::write(
        activity_path(book_root),
        if body.is_empty() {
            body
        } else {
            format!("{body}\n")
        },
    )?;
    Ok(())
}

fn activity_path(book_root: &Path) -> std::path::PathBuf {
    layout::sync_dir(book_root).join(ACTIVITY_FILE)
}

fn latest_time(current: Option<DateTime<Utc>>, candidate: DateTime<Utc>) -> Option<DateTime<Utc>> {
    Some(match current {
        Some(current) if current > candidate => current,
        _ => candidate,
    })
}

fn note_id_from_activity_path(path: &str) -> Option<NoteId> {
    let file_name = path.rsplit('/').next()?;
    let stem = file_name.strip_suffix(".md").unwrap_or(file_name);
    let stem = stem
        .split(".conflict-")
        .next()
        .unwrap_or(stem)
        .split(".sb-")
        .next()
        .unwrap_or(stem);
    NoteId::parse(stem).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ObjectType;

    #[test]
    fn activity_summary_counts_unique_visible_notes_and_latest_times() {
        let now = Utc::now();
        let note_id = NoteId::generate(ObjectType::Note.id_prefix(), "tasks");
        let note_path = format!("{}.md", note_id.as_str());
        let old_path = "note-old-01HX0J0Q2MZP9GE23YB8D7FZ9K.md".to_string();
        let events = vec![
            event_at(
                now - Duration::minutes(5),
                "external_update",
                Some(&note_path),
            ),
            event_at(
                now - Duration::minutes(4),
                "external_update",
                Some(&note_path),
            ),
            event_at(now - Duration::days(2), "external_update", Some(&old_path)),
            event_at(
                now - Duration::minutes(3),
                "conflict_detected",
                Some(&note_path),
            ),
            event_at(
                now - Duration::minutes(2),
                "remote_loro_merge",
                Some(&note_path),
            ),
        ];

        let summary = summarize_activity(&events, now);

        assert_eq!(summary.external_updates_24h, 1);
        assert_eq!(summary.external_updates_7d, 2);
        assert_eq!(summary.external_note_updates_24h, 1);
        assert_eq!(summary.conflict_copies_7d, 1);
        assert_eq!(summary.remote_loro_merges_7d, 1);
        assert_eq!(
            summary.latest_external_update_at,
            Some(now - Duration::minutes(4))
        );
    }

    #[test]
    fn latest_note_activity_prefers_newest_relevant_event() {
        let now = Utc::now();
        let note_id = NoteId::generate(ObjectType::Note.id_prefix(), "merge");
        let note_path = format!("{}.md", note_id.as_str());
        let events = vec![
            event_at(
                now - Duration::minutes(10),
                "external_update",
                Some(&note_path),
            ),
            event_at(
                now - Duration::minutes(1),
                "remote_loro_merge",
                Some(&note_path),
            ),
        ];

        let latest = latest_note_activity(&events, &note_id, now).unwrap();

        assert_eq!(latest.kind, "remote_loro_merge");
        assert_eq!(latest.happened_at, now - Duration::minutes(1));
    }

    fn event_at(happened_at: DateTime<Utc>, kind: &str, path: Option<&str>) -> SyncActivityEvent {
        SyncActivityEvent {
            happened_at,
            source: "test".to_string(),
            kind: kind.to_string(),
            path: path.map(str::to_string),
            detail: "detail".to_string(),
        }
    }
}
