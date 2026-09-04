use api::routes::intake::{
    validate_create, validate_issue_create, CreateIntake, CreateIntakeIssue, IntakeIssuePayload,
};

#[test]
fn rejects_empty_name() {
    let i = CreateIntake {
        name: "  ".to_string(),
        description: None,
    };
    let err = validate_create(&i).unwrap_err();
    assert!(err.to_lowercase().contains("name"));
}

#[test]
fn rejects_name_over_255() {
    let i = CreateIntake {
        name: "x".repeat(256),
        description: None,
    };
    let err = validate_create(&i).unwrap_err();
    assert!(err.to_lowercase().contains("255"));
}

#[test]
fn accepts_valid_intake() {
    let i = CreateIntake {
        name: "Mobile requests".to_string(),
        description: None,
    };
    assert!(validate_create(&i).is_ok());
}

#[test]
fn issue_rejects_missing_name() {
    let ii = CreateIntakeIssue {
        issue: IntakeIssuePayload {
            name: None,
            priority: None,
        },
    };
    let err = validate_issue_create(&ii).unwrap_err();
    assert!(err.to_lowercase().contains("name is required"));
}

#[test]
fn issue_rejects_invalid_priority() {
    let ii = CreateIntakeIssue {
        issue: IntakeIssuePayload {
            name: Some("Bug".to_string()),
            priority: Some("critical".to_string()),
        },
    };
    let err = validate_issue_create(&ii).unwrap_err();
    assert!(err.to_lowercase().contains("invalid priority"));
}

#[test]
fn issue_accepts_valid() {
    for p in [None, Some("low"), Some("medium"), Some("high"), Some("urgent"), Some("none")] {
        let ii = CreateIntakeIssue {
            issue: IntakeIssuePayload {
                name: Some("Bug".to_string()),
                priority: p.map(str::to_string),
            },
        };
        assert!(validate_issue_create(&ii).is_ok());
    }
}
