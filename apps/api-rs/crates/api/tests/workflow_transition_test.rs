use api::routes::workflow::{evaluate_transition, TransitionContext};
use uuid::Uuid;

#[test]
fn same_state_is_noop() {
    let ctx = TransitionContext {
        pairs: vec![],
        transitions: vec![],
        default_state_id: None,
    };
    let state = Uuid::new_v4();
    assert!(evaluate_transition(state, state, &ctx).is_ok());
}

#[test]
fn transition_must_exist() {
    let mirror_new = Uuid::new_v4();
    let mirror_progress = Uuid::new_v4();
    let mirror_closed = Uuid::new_v4();
    let wf_new = Uuid::new_v4();
    let wf_progress = Uuid::new_v4();
    let wf_closed = Uuid::new_v4();
    let ctx = TransitionContext {
        pairs: vec![
            (mirror_new, wf_new),
            (mirror_progress, wf_progress),
            (mirror_closed, wf_closed),
        ],
        transitions: vec![(wf_new, wf_progress), (wf_progress, wf_closed)],
        default_state_id: Some(mirror_new),
    };
    assert!(evaluate_transition(mirror_new, mirror_progress, &ctx).is_ok());
    assert!(evaluate_transition(mirror_progress, mirror_closed, &ctx).is_ok());
    assert_eq!(
        evaluate_transition(mirror_new, mirror_closed, &ctx),
        Err(vec![mirror_progress])
    );
    assert_eq!(
        evaluate_transition(mirror_closed, mirror_new, &ctx),
        Err(vec![])
    );
    // Target di luar workflow type → tidak ada yang diizinkan.
    assert_eq!(
        evaluate_transition(mirror_new, Uuid::new_v4(), &ctx),
        Err(vec![])
    );
}

#[test]
fn legacy_current_state_only_moves_to_default() {
    let mirror_new = Uuid::new_v4();
    let legacy_state = Uuid::new_v4();
    let wf_new = Uuid::new_v4();
    let ctx = TransitionContext {
        pairs: vec![(mirror_new, wf_new)],
        transitions: vec![],
        default_state_id: Some(mirror_new),
    };
    assert!(evaluate_transition(legacy_state, mirror_new, &ctx).is_ok());
    assert_eq!(
        evaluate_transition(legacy_state, Uuid::new_v4(), &ctx),
        Err(vec![mirror_new])
    );
}

#[test]
fn legacy_without_default_denies_everything() {
    let mirror_new = Uuid::new_v4();
    let legacy_state = Uuid::new_v4();
    let ctx = TransitionContext {
        pairs: vec![(mirror_new, Uuid::new_v4())],
        transitions: vec![],
        default_state_id: None,
    };
    assert_eq!(
        evaluate_transition(legacy_state, mirror_new, &ctx),
        Err(vec![])
    );
}

#[test]
fn legacy_target_inside_workflow_still_must_be_default() {
    let mirror_new = Uuid::new_v4();
    let mirror_progress = Uuid::new_v4();
    let legacy_state = Uuid::new_v4();
    let wf_new = Uuid::new_v4();
    let wf_progress = Uuid::new_v4();
    let ctx = TransitionContext {
        pairs: vec![(mirror_new, wf_new), (mirror_progress, wf_progress)],
        transitions: vec![(wf_new, wf_progress)],
        default_state_id: Some(mirror_new),
    };
    assert_eq!(
        evaluate_transition(legacy_state, mirror_progress, &ctx),
        Err(vec![mirror_new])
    );
}
