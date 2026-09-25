use api::routes::workflow::{allowed_target_state_ids, validate_name, validate_state_group};
use uuid::Uuid;

#[test]
fn name_required_and_max_255() {
    assert!(validate_name("  ", "name").is_err());
    assert!(validate_name(&"x".repeat(256), "name").is_err());
    assert_eq!(validate_name(" Incident ", "name").unwrap(), "Incident");
}

#[test]
fn name_accepts_exactly_255_chars() {
    assert_eq!(
        validate_name(&"x".repeat(255), "name").unwrap(),
        "x".repeat(255)
    );
    assert!(validate_name(&"x".repeat(256), "name").is_err());
}

#[test]
fn name_length_counts_chars_not_bytes() {
    let name = "é".repeat(200);
    assert_eq!(name.len(), 400);
    assert_eq!(validate_name(&name, "name").unwrap(), name);
}

#[test]
fn group_must_be_known() {
    assert!(validate_state_group("backlog").is_ok());
    assert!(validate_state_group("triage").is_err());
    assert!(validate_state_group("bogus").is_err());
}

#[test]
fn group_error_names_offending_value() {
    assert_eq!(
        validate_state_group("bogus").unwrap_err(),
        "\"bogus\" is not a valid choice."
    );
    assert_eq!(
        validate_state_group("triage").unwrap_err(),
        "\"triage\" is not a valid choice."
    );
}

#[test]
fn allowed_targets_follow_transitions() {
    let mirror_new = Uuid::new_v4();
    let mirror_progress = Uuid::new_v4();
    let mirror_closed = Uuid::new_v4();
    let wf_new = Uuid::new_v4();
    let wf_progress = Uuid::new_v4();
    let wf_closed = Uuid::new_v4();
    let pairs = vec![
        (mirror_new, wf_new),
        (mirror_progress, wf_progress),
        (mirror_closed, wf_closed),
    ];
    let transitions = vec![(wf_new, wf_progress), (wf_progress, wf_closed)];

    assert_eq!(
        allowed_target_state_ids(mirror_new, &pairs, &transitions),
        vec![mirror_progress]
    );
    assert_eq!(
        allowed_target_state_ids(mirror_progress, &pairs, &transitions),
        vec![mirror_closed]
    );
    assert!(allowed_target_state_ids(mirror_closed, &pairs, &transitions).is_empty());
    assert!(allowed_target_state_ids(Uuid::new_v4(), &pairs, &transitions).is_empty());
}

#[test]
fn allowed_targets_do_not_duplicate_on_repeated_transitions() {
    let mirror_new = Uuid::new_v4();
    let mirror_progress = Uuid::new_v4();
    let wf_new = Uuid::new_v4();
    let wf_progress = Uuid::new_v4();
    let pairs = vec![(mirror_new, wf_new), (mirror_progress, wf_progress)];
    let transitions = vec![(wf_new, wf_progress), (wf_new, wf_progress)];

    assert_eq!(
        allowed_target_state_ids(mirror_new, &pairs, &transitions),
        vec![mirror_progress]
    );
}

#[test]
fn allowed_targets_exclude_unmapped_workflow_state() {
    let mirror_new = Uuid::new_v4();
    let mirror_progress = Uuid::new_v4();
    let wf_new = Uuid::new_v4();
    let wf_progress = Uuid::new_v4();
    let wf_orphan = Uuid::new_v4();
    let pairs = vec![(mirror_new, wf_new), (mirror_progress, wf_progress)];
    let transitions = vec![(wf_new, wf_progress), (wf_new, wf_orphan)];

    assert_eq!(
        allowed_target_state_ids(mirror_new, &pairs, &transitions),
        vec![mirror_progress]
    );
}
