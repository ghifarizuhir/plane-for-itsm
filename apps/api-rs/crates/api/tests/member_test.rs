use api::routes::member::{
    validate_create, validate_invite_create, CreateMember, CreateInvite,
};

#[test]
fn rejects_invalid_role() {
    let m = CreateMember {
        member: Some(uuid::Uuid::new_v4()),
        role: Some(99),
    };
    let err = validate_create(&m).unwrap_err();
    assert!(err.to_lowercase().contains("invalid role"));
}

#[test]
fn rejects_missing_member() {
    let m = CreateMember {
        member: None,
        role: Some(15),
    };
    let err = validate_create(&m).unwrap_err();
    assert!(err.to_lowercase().contains("member is required"));
}

#[test]
fn accepts_valid_roles() {
    for r in [None, Some(20), Some(15), Some(5)] {
        let m = CreateMember {
            member: Some(uuid::Uuid::new_v4()),
            role: r,
        };
        assert!(validate_create(&m).is_ok());
    }
}

#[test]
fn invite_rejects_invalid_email() {
    let i = CreateInvite {
        email: "not-an-email".to_string(),
        role: Some(15),
    };
    let err = validate_invite_create(&i).unwrap_err();
    assert!(err.to_lowercase().contains("invalid email"));
}

#[test]
fn invite_rejects_invalid_role() {
    let i = CreateInvite {
        email: "a@example.com".to_string(),
        role: Some(42),
    };
    let err = validate_invite_create(&i).unwrap_err();
    assert!(err.to_lowercase().contains("invalid role"));
}

#[test]
fn invite_accepts_valid() {
    let i = CreateInvite {
        email: "a@example.com".to_string(),
        role: None,
    };
    assert!(validate_invite_create(&i).is_ok());
}
