use api::routes::work_item::{
    validate_link_create, validate_relation_create, CreateLink, CreateRelation,
};

#[test]
fn link_rejects_missing_url() {
    let l = CreateLink {
        title: Some("Docs".to_string()),
        url: "  ".to_string(),
    };
    let err = validate_link_create(&l).unwrap_err();
    assert!(err.to_lowercase().contains("url"));
}

#[test]
fn link_rejects_title_over_255() {
    let l = CreateLink {
        title: Some("x".repeat(256)),
        url: "https://example.com".to_string(),
    };
    let err = validate_link_create(&l).unwrap_err();
    assert!(err.contains("255"));
}

#[test]
fn link_accepts_valid() {
    let l = CreateLink {
        title: None,
        url: "https://example.com/spec".to_string(),
    };
    assert!(validate_link_create(&l).is_ok());
}

#[test]
fn relation_rejects_invalid_type() {
    let r = CreateRelation {
        issues: vec![uuid::Uuid::new_v4()],
        relation_type: Some("eats".to_string()),
    };
    let err = validate_relation_create(&r).unwrap_err();
    assert!(err.to_lowercase().contains("relation type"));
}

#[test]
fn relation_rejects_empty_issues() {
    let r = CreateRelation {
        issues: vec![],
        relation_type: Some("blocked_by".to_string()),
    };
    let err = validate_relation_create(&r).unwrap_err();
    assert!(err.to_lowercase().contains("at least one"));
}

#[test]
fn relation_accepts_valid_types() {
    for t in ["blocking", "blocked_by", "duplicate", "relates_to", "start_before", "start_after", "finish_before", "finish_after"] {
        let r = CreateRelation {
            issues: vec![uuid::Uuid::new_v4()],
            relation_type: Some(t.to_string()),
        };
        assert!(validate_relation_create(&r).is_ok(), "type={t}");
    }
}
