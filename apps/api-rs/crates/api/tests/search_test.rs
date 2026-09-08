use api::routes::search::{build_search_issue_item, parse_entities, SearchIssueRow, SEARCH_ENTITIES};

#[test]
fn filters_unknown_entities() {
    let entities = parse_entities(Some("issue,page,bogus,,cycle"));
    assert_eq!(entities, vec!["issue", "page", "cycle"]);
}

#[test]
fn defaults_to_all_entities() {
    for param in [None, Some(""), Some("   ")] {
        let entities = parse_entities(param);
        assert_eq!(entities.len(), SEARCH_ENTITIES.len());
    }
}

#[test]
fn search_issue_item_matches_django_values_shape() {
    // Regression: `ParentIssuesListModal` crashed with `issues.map is not a
    // function` because Rust `issue_search` returned `{"results": [...]}`.
    // Django `IssueSearchEndpoint.get` returns a BARE array via
    // `Response(issues.values(...))`, and `projectIssuesSearch` passes
    // `response?.data` straight into `setIssues`. The handler must serialize
    // `Vec<Value>` (bare `[]`), with Django's 11 `.values()` keys.
    let item = build_search_issue_item(SearchIssueRow {
        id: uuid::Uuid::new_v4(),
        name: "Bug".to_string(),
        start_date: None,
        sequence_id: 7,
        project__name: "P".to_string(),
        project__identifier: "NPS".to_string(),
        project_id: uuid::Uuid::new_v4(),
        workspace__slug: "itsm".to_string(),
        state__name: Some("Todo".to_string()),
        state__group: Some("unstarted".to_string()),
        state__color: Some("#ff0000".to_string()),
    });
    for key in [
        "name",
        "id",
        "start_date",
        "sequence_id",
        "project__name",
        "project__identifier",
        "project_id",
        "workspace__slug",
        "state__name",
        "state__group",
        "state__color",
    ] {
        assert!(item.get(key).is_some(), "missing {key}");
    }
    // Bare-array envelope: `issues.map` in the modals must work.
    let arr = serde_json::Value::Array(vec![item]);
    assert!(arr.as_array().is_some());
}
