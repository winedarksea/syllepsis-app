//! The sync **planner**: a pure function from (local fingerprints, remote listing, last-sync state)
//! to a list of [`SyncAction`]s. No I/O and no CRDT — just the decision matrix — so the trickiest
//! part of sync (who-changed-what, and the loop-prevention rules) is exhaustively unit-testable.
//!
//! For each path the planner asks two questions against [`SyncState`]: did it change locally, and
//! did it change on the remote? The four answers, plus presence/absence on each side, fully
//! determine the action. The headline cases:
//! - changed on exactly one side → push or pull;
//! - changed on both sides and the file is a **CRDT sidecar** → [`Merge`](SyncAction::Merge) (the
//!   convergent path — this is why notes can be edited on two devices at once);
//! - changed on both sides and it is *not* mergeable → [`Conflict`](SyncAction::Conflict) copy;
//! - changed on neither side → [`Skip`](SyncAction::Skip), so a quiet file never causes a write.

use std::collections::{BTreeMap, BTreeSet};

use crate::sync::state::SyncState;

/// What to do with one path this sync pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAction {
    /// Upload the local bytes to the remote (local is new or the only side that changed).
    Push(String),
    /// Download the remote bytes over the local file (remote is new or the only side that changed).
    Pull(String),
    /// Both sides changed a CRDT sidecar: merge the two snapshots (commutative, lossless).
    Merge(String),
    /// Both sides changed a non-mergeable file: keep local, drop a `.conflict-*` copy of the remote
    /// for the user to reconcile.
    Conflict(String),
    /// The remote dropped a file we had synced and not since edited locally — delete it here too.
    DeleteLocal(String),
    /// We deleted a file locally that the remote still has and has not since edited — delete it
    /// there too.
    DeleteRemote(String),
    /// Nothing changed on either side. Recorded (not silent) so the engine can refresh state and so
    /// tests can assert the no-op behavior that prevents write loops.
    Skip(String),
}

impl SyncAction {
    pub fn path(&self) -> &str {
        match self {
            SyncAction::Push(p)
            | SyncAction::Pull(p)
            | SyncAction::Merge(p)
            | SyncAction::Conflict(p)
            | SyncAction::DeleteLocal(p)
            | SyncAction::DeleteRemote(p)
            | SyncAction::Skip(p) => p,
        }
    }
}

/// Compute the action for every path present locally or remotely.
///
/// `local` maps each syncable book-relative path to its current content hash; `remote` maps each
/// remote path to its revision; `mergeable` reports whether a path is a CRDT sidecar (the only
/// kind we merge rather than conflict). Paths absent from both sides are not returned — the engine
/// prunes their stale state separately.
pub fn plan(
    local: &BTreeMap<String, String>,
    remote: &BTreeMap<String, String>,
    state: &SyncState,
    mergeable: impl Fn(&str) -> bool,
) -> Vec<SyncAction> {
    let paths: BTreeSet<&String> = local.keys().chain(remote.keys()).collect();
    paths
        .into_iter()
        .map(|path| decide(path, local.get(path), remote.get(path), state, &mergeable))
        .collect()
}

fn decide(
    path: &str,
    local: Option<&String>,
    remote: Option<&String>,
    state: &SyncState,
    mergeable: &impl Fn(&str) -> bool,
) -> SyncAction {
    let prior = state.get(path);
    match (local, remote) {
        (Some(local_hash), Some(remote_rev)) => {
            // Fast path for content-hash providers: identical bytes on both sides are already in
            // sync regardless of history, so never manufacture a conflict from equal content.
            if local_hash == remote_rev {
                return SyncAction::Skip(path.to_string());
            }
            let local_changed = prior.map(|s| &s.local_hash != local_hash).unwrap_or(true);
            let remote_changed = prior
                .map(|s| &s.remote_revision != remote_rev)
                .unwrap_or(true);
            match (local_changed, remote_changed) {
                (false, false) => SyncAction::Skip(path.to_string()),
                (true, false) => SyncAction::Push(path.to_string()),
                (false, true) => SyncAction::Pull(path.to_string()),
                (true, true) if mergeable(path) => SyncAction::Merge(path.to_string()),
                (true, true) => SyncAction::Conflict(path.to_string()),
            }
        }
        (Some(local_hash), None) => match prior {
            // Synced before, gone from remote: honor the remote deletion unless we have a local edit
            // since (in which case keep our work and re-push it).
            Some(s) if &s.local_hash == local_hash => SyncAction::DeleteLocal(path.to_string()),
            _ => SyncAction::Push(path.to_string()),
        },
        (None, Some(remote_rev)) => match prior {
            // Synced before, gone locally: honor our deletion unless the remote edited since.
            Some(s) if &s.remote_revision == remote_rev => {
                SyncAction::DeleteRemote(path.to_string())
            }
            _ => SyncAction::Pull(path.to_string()),
        },
        // Absent both sides: not reachable (planner only visits the union of present paths).
        (None, None) => SyncAction::Skip(path.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn never_mergeable(_: &str) -> bool {
        false
    }

    #[test]
    fn new_local_file_is_pushed_new_remote_is_pulled() {
        let state = SyncState::new("p");
        let actions = plan(
            &map(&[("a.md", "ha")]),
            &map(&[("b.md", "rb")]),
            &state,
            never_mergeable,
        );
        assert!(actions.contains(&SyncAction::Push("a.md".into())));
        assert!(actions.contains(&SyncAction::Pull("b.md".into())));
    }

    #[test]
    fn unchanged_on_both_sides_is_skipped() {
        let mut state = SyncState::new("p");
        state.mark_synced("a.md", "ha", "ra");
        let actions = plan(
            &map(&[("a.md", "ha")]),
            &map(&[("a.md", "ra")]),
            &state,
            never_mergeable,
        );
        assert_eq!(actions, vec![SyncAction::Skip("a.md".into())]);
    }

    #[test]
    fn one_sided_change_pushes_or_pulls() {
        let mut state = SyncState::new("p");
        state.mark_synced("a.md", "ha", "ra");
        state.mark_synced("b.md", "hb", "rb");
        // a changed locally only; b changed remotely only.
        let actions = plan(
            &map(&[("a.md", "ha2"), ("b.md", "hb")]),
            &map(&[("a.md", "ra"), ("b.md", "rb2")]),
            &state,
            never_mergeable,
        );
        assert!(actions.contains(&SyncAction::Push("a.md".into())));
        assert!(actions.contains(&SyncAction::Pull("b.md".into())));
    }

    #[test]
    fn both_changed_merges_sidecars_but_conflicts_other_files() {
        let mut state = SyncState::new("p");
        state.mark_synced("_crdt/x.crdt", "h0", "r0");
        state.mark_synced("note.md", "h0", "r0");
        let is_sidecar = |p: &str| p.starts_with("_crdt/");
        let actions = plan(
            &map(&[("_crdt/x.crdt", "h1"), ("note.md", "h1")]),
            &map(&[("_crdt/x.crdt", "r1"), ("note.md", "r1")]),
            &state,
            is_sidecar,
        );
        assert!(actions.contains(&SyncAction::Merge("_crdt/x.crdt".into())));
        assert!(actions.contains(&SyncAction::Conflict("note.md".into())));
    }

    #[test]
    fn remote_deletion_removes_local_only_when_local_unchanged() {
        let mut state = SyncState::new("p");
        state.mark_synced("gone.md", "h0", "r0");
        state.mark_synced("kept.md", "h0", "r0");
        // gone.md unchanged locally → delete; kept.md edited locally → resurrect via push.
        let actions = plan(
            &map(&[("gone.md", "h0"), ("kept.md", "h1")]),
            &map(&[]),
            &state,
            never_mergeable,
        );
        assert!(actions.contains(&SyncAction::DeleteLocal("gone.md".into())));
        assert!(actions.contains(&SyncAction::Push("kept.md".into())));
    }

    #[test]
    fn local_deletion_removes_remote_only_when_remote_unchanged() {
        let mut state = SyncState::new("p");
        state.mark_synced("gone.md", "h0", "r0");
        state.mark_synced("edited.md", "h0", "r0");
        // gone.md: remote unchanged → delete remote; edited.md: remote changed → resurrect via pull.
        let actions = plan(
            &map(&[]),
            &map(&[("gone.md", "r0"), ("edited.md", "r1")]),
            &state,
            never_mergeable,
        );
        assert!(actions.contains(&SyncAction::DeleteRemote("gone.md".into())));
        assert!(actions.contains(&SyncAction::Pull("edited.md".into())));
    }

    #[test]
    fn identical_content_never_conflicts() {
        // Same content hash on both sides with no prior state → already in sync, not a conflict.
        let state = SyncState::new("p");
        let actions = plan(
            &map(&[("note.md", "same")]),
            &map(&[("note.md", "same")]),
            &state,
            never_mergeable,
        );
        assert_eq!(actions, vec![SyncAction::Skip("note.md".into())]);
    }
}
