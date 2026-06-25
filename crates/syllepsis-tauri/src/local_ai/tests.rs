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
    assert!(!note_generation_blocked(
        &policy,
        &PowerSource::Battery
    ));
}
