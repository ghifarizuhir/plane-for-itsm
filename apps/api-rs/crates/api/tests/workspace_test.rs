use api::routes::workspace::{validate_create, CreateWorkspace};

fn valid_payload() -> CreateWorkspace {
    CreateWorkspace {
        name: "My Workspace".to_string(),
        slug: "my-workspace".to_string(),
    }
}

#[test]
fn rejects_empty_name_and_slug() {
    let payload = CreateWorkspace {
        name: "".to_string(),
        slug: "".to_string(),
    };
    let err = validate_create(&payload).unwrap_err();
    assert!(err.contains("name") || err.contains("slug"));
}

#[test]
fn rejects_name_over_80_and_slug_over_48() {
    let payload = CreateWorkspace {
        name: "a".repeat(81),
        slug: "s".repeat(49),
    };
    assert!(validate_create(&payload).is_err());
}

#[test]
fn rejects_name_containing_url() {
    let payload = CreateWorkspace {
        name: "Check http://example.com".to_string(),
        slug: "ok-slug".to_string(),
    };
    let err = validate_create(&payload).unwrap_err();
    assert!(err.to_lowercase().contains("url"));
}

#[test]
fn rejects_restricted_slug() {
    for slug in ["api", "admin", "god-mode", "spaces"] {
        let payload = CreateWorkspace {
            name: "Ok Name".to_string(),
            slug: slug.to_string(),
        };
        assert!(
            validate_create(&payload).is_err(),
            "slug {slug} must be rejected"
        );
    }
}

#[test]
fn rejects_slug_with_bad_chars() {
    let payload = CreateWorkspace {
        name: "Ok".to_string(),
        slug: "bad slug!".to_string(),
    };
    assert!(validate_create(&payload).is_err());
}

#[test]
fn accepts_valid_payload() {
    assert!(validate_create(&valid_payload()).is_ok());
}
