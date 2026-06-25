use super::*;

#[test]
fn disabled_generation_blocks_only_background_notes() {
    let policy = LocalAiDevicePolicy {
        generate_note_embeddings: false,
        ..LocalAiDevicePolicy::default()
    };
    assert!(note_generation_blocked(&policy, &PowerSource::Ac));
}

#[test]
fn battery_policy_is_configurable() {
    let mut policy = LocalAiDevicePolicy::default();
    assert!(note_generation_blocked(&policy, &PowerSource::Battery));
    policy.pause_note_embeddings_on_battery = false;
    assert!(!note_generation_blocked(&policy, &PowerSource::Battery));
}

#[test]
fn unavailable_model_blocks_the_whole_background_queue_with_one_reason() {
    let worker = LocalAiWorker::new();
    {
        let mut state = worker.shared.state.lock().unwrap();
        let root = PathBuf::from("/tmp/book");
        for index in 0..3 {
            let note_id = format!("note-{index}");
            state.note_jobs.insert(
                note_job_key(&root, &note_id),
                NoteJob {
                    book_root: root.clone(),
                    models_root: PathBuf::from("/tmp/models"),
                    note_id,
                    due_at: Instant::now(),
                },
            );
        }
        state.note_block_reason = Some("embedding model is not downloaded".into());
    }

    let status = worker.status();
    assert_eq!(status.pending_note_jobs, 3);
    assert_eq!(status.blocked_note_jobs, 3);
    assert_eq!(
        status.note_block_reason.as_deref(),
        Some("embedding model is not downloaded")
    );
}
