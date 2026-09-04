use api::routes::project::{guard_identifier_unique, guard_name_unique, guard_patch};

#[test]
fn project_patch_rejects_archived() {
    let err = guard_patch(true).unwrap_err();
    assert!(err.contains("Archived projects cannot be updated"));
    assert!(guard_patch(false).is_ok());
}

#[test]
fn project_patch_rejects_duplicate_name() {
    let err = guard_name_unique(true).unwrap_err();
    assert!(err.contains("PROJECT_NAME_ALREADY_EXIST"));
    assert!(guard_name_unique(false).is_ok());
}

#[test]
fn project_patch_rejects_duplicate_identifier() {
    let err = guard_identifier_unique(true).unwrap_err();
    assert!(err.contains("PROJECT_IDENTIFIER_ALREADY_EXIST"));
    assert!(guard_identifier_unique(false).is_ok());
}
