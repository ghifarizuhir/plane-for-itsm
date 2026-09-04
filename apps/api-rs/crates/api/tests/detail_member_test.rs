use api::routes::member::{guard_destroy_role, guard_destroy_self, guard_patch_self};
use api::routes::view::guard_guest_access;

#[test]
fn member_patch_rejects_self_role_update() {
    let err = guard_patch_self(true, false).unwrap_err();
    assert!(err.contains("You cannot update your own role"));
    assert!(guard_patch_self(false, false).is_ok());
    assert!(guard_patch_self(true, true).is_ok());
}

#[test]
fn member_delete_rejects_self_removal() {
    let err = guard_destroy_self(true).unwrap_err();
    assert!(err.contains("You cannot remove yourself"));
    assert!(guard_destroy_self(false).is_ok());
}

#[test]
fn member_delete_rejects_higher_role_target() {
    let err = guard_destroy_role(15, 20).unwrap_err();
    assert!(err.contains("You cannot remove a user having role higher than you"));
    assert!(guard_destroy_role(20, 15).is_ok());
    assert!(guard_destroy_role(15, 15).is_ok());
}

#[test]
fn view_rejects_guest_without_access() {
    let err = guard_guest_access(true, false, false).unwrap_err();
    assert!(err.contains("You are not allowed to view this issue"));
    assert!(guard_guest_access(true, true, false).is_ok());
    assert!(guard_guest_access(true, false, true).is_ok());
    assert!(guard_guest_access(false, false, false).is_ok());
}
