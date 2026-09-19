use api::routes::v1::work_item::{
    count_group_column, v1_count_json, v1_search_issue_json, v1_work_item_json, V1SearchRow,
};
use api::routes::issue_common::IssueListRow;
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

fn list_row() -> IssueListRow {
    IssueListRow {
        id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
        name: "Fix".to_string(),
        state_id: None,
        sort_order: 1.0,
        completed_at: None,
        estimate_point: None,
        priority: "urgent".to_string(),
        start_date: None,
        target_date: None,
        sequence_id: 3,
        project_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
        parent_id: None,
        cycle_id: None,
        module_ids: vec![],
        label_ids: vec![Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap()],
        assignee_ids: vec![Uuid::parse_str("44444444-4444-4444-4444-444444444444").unwrap()],
        sub_issues_count: 0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        created_by: None,
        updated_by: None,
        attachment_count: 0,
        link_count: 0,
        is_draft: false,
        archived_at: None,
        deleted_at: None,
    }
}

#[test]
fn work_item_json_adds_assignees_and_labels_aliases() {
    let v = v1_work_item_json(&list_row());
    let o = v.as_object().unwrap();
    assert!(o.contains_key("id"));
    assert!(o.contains_key("name"));
    assert_eq!(o["assignees"].as_array().unwrap().len(), 1);
    assert_eq!(o["labels"].as_array().unwrap().len(), 1);
    assert!(o.contains_key("assignee_ids"));
}

#[test]
fn count_json_no_grouping() {
    let v = v1_count_json(None, None, 7, vec![]);
    assert_eq!(v["total_count"], Value::from(7));
    assert_eq!(v["grouped_by"], Value::Null);
    assert_eq!(v["grouped_counts"], serde_json::json!({}));
}

#[test]
fn count_json_flat_grouping() {
    let v = v1_count_json(Some("priority"), None, 7, vec![("urgent".into(), 2), ("None".into(), 5)]);
    assert_eq!(v["grouped_by"], Value::from("priority"));
    assert_eq!(v["grouped_counts"]["urgent"]["count"], Value::from(2));
    assert_eq!(v["grouped_counts"]["None"]["count"], Value::from(5));
}

#[test]
fn count_group_column_allowlist() {
    assert_eq!(count_group_column("state_id"), Some("i.state_id::text"));
    assert_eq!(count_group_column("state__group"), Some("s.\"group\""));
    assert_eq!(count_group_column("priority"), Some("i.priority"));
    assert_eq!(count_group_column("project_id"), Some("i.project_id::text"));
    assert_eq!(count_group_column("type_id"), Some("i.type_id::text"));
    assert_eq!(count_group_column("created_by"), Some("i.created_by_id::text"));
    assert_eq!(count_group_column("target_date"), Some("i.target_date::text"));
    assert_eq!(count_group_column("start_date"), Some("i.start_date::text"));
    assert_eq!(count_group_column("cycle_id"), None);
    assert_eq!(count_group_column("label_ids"), None);
}

#[test]
fn search_issue_json_has_sdk_required_fields() {
    let row = V1SearchRow {
        id: Uuid::nil(),
        name: "n".into(),
        sequence_id: 1,
        project_id: Uuid::nil(),
        project_identifier: "ENG".into(),
        workspace_slug: "itsm".into(),
    };
    let v = v1_search_issue_json(&row);
    for key in ["id", "name", "sequence_id", "project_id", "project__identifier", "workspace__slug"] {
        assert!(v.get(key).is_some(), "missing {key}");
    }
}
