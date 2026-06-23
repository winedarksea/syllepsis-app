//! Device-local sync activity log for UI observability.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::CoreResult;
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
