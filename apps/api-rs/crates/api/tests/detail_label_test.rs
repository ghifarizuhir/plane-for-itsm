use api::routes::estimate::guard_patch as estimate_guard_patch;
use api::routes::intake::guard_delete as intake_guard_delete;
use api::routes::label::guard_patch as label_guard_patch;

#[test]
fn label_patch_rejects_duplicate_name() {
    let err = label_guard_patch(true).unwrap_err();
    assert!(err.contains("Label with the same name already exists in the project"));
    assert!(label_guard_patch(false).is_ok());
}

#[test]
fn estimate_patch_requires_points() {
    let err = estimate_guard_patch(true).unwrap_err();
    assert!(err.contains("Estimate points are required"));
    assert!(estimate_guard_patch(false).is_ok());
}

#[test]
fn intake_delete_rejects_default() {
    let err = intake_guard_delete(true).unwrap_err();
    assert!(err.contains("You cannot delete the default intake"));
    assert!(intake_guard_delete(false).is_ok());
}
