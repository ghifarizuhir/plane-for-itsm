use api::routes::cycle::{guard_patch, validate_create, CreateCycle};
use api::routes::module::guard_patch as module_guard_patch;
use api::routes::state::guard_delete as state_guard_delete;
use chrono::{Duration, Utc};

#[test]
fn cycle_rejects_partial_dates() {
    let c = CreateCycle {
        name: "S".to_string(),
        start_date: Some(Utc::now()),
        end_date: None,
    };
    let err = validate_create(&c).unwrap_err();
    assert!(err.to_lowercase().contains("start date and end date"));
}

#[test]
fn cycle_patch_rejects_archived() {
    let err = guard_patch(true, false, false).unwrap_err();
    assert!(err.contains("Archived cycle cannot be updated"));
}

#[test]
fn cycle_patch_rejects_completed_for_regular_fields() {
    let err = guard_patch(false, true, false).unwrap_err();
    assert!(err.contains("already been completed"));
}

#[test]
fn cycle_patch_allows_sort_order_on_completed() {
    assert!(guard_patch(false, true, true).is_ok());
    assert!(guard_patch(false, false, false).is_ok());
}

#[test]
fn module_patch_rejects_archived() {
    let err = module_guard_patch(true).unwrap_err();
    assert!(err.contains("Archived module cannot be updated"));
    assert!(module_guard_patch(false).is_ok());
}

#[test]
fn state_delete_rejects_default() {
    let err = state_guard_delete(true, 0).unwrap_err();
    assert!(err.contains("Default state cannot be deleted"));
}

#[test]
fn state_delete_rejects_non_empty() {
    let err = state_guard_delete(false, 3).unwrap_err();
    assert!(err.contains("only empty states can be deleted"));
    assert!(state_guard_delete(false, 0).is_ok());
}

#[test]
fn cycle_accepts_both_dates_null() {
    let c = CreateCycle {
        name: "S".to_string(),
        start_date: None,
        end_date: None,
    };
    assert!(validate_create(&c).is_ok());
    let now = Utc::now();
    let c = CreateCycle {
        name: "S".to_string(),
        start_date: Some(now),
        end_date: Some(now + Duration::days(1)),
    };
    assert!(validate_create(&c).is_ok());
}
