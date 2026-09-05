use api::routes::project::{validate_create, CreateProject};

#[test]
fn rejects_empty_name() {
    let p = CreateProject {
        name: "".to_string(),
        identifier: "ABC".to_string(),
        project_lead: None,
    };
    assert!(validate_create(&p).is_err());
}

#[test]
fn rejects_name_with_forbidden_chars() {
    // Mirrors Project.FORBIDDEN_IDENTIFIER_CHARS_PATTERN
    for bad in ["a&b", "x@y", "p!q", "a/b", "x(y)"] {
        let p = CreateProject {
            name: bad.to_string(),
            identifier: "ABC".to_string(),
            project_lead: None,
        };
        assert!(validate_create(&p).is_err(), "name {bad} must be rejected");
    }
}

#[test]
fn rejects_bad_identifier() {
    // empty, too long (>12), forbidden chars
    for bad in ["", "ABCDEFGHIJKLM", "A&B", "a@b"] {
        let p = CreateProject {
            name: "Good Name".to_string(),
            identifier: bad.to_string(),
            project_lead: None,
        };
        assert!(
            validate_create(&p).is_err(),
            "identifier {bad:?} must be rejected"
        );
    }
}

#[test]
fn accepts_valid_project() {
    let p = CreateProject {
        name: "Backend".to_string(),
        identifier: "BE".to_string(),
        project_lead: None,
    };
    assert!(validate_create(&p).is_ok());
}
