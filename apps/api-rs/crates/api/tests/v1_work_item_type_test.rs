use api::routes::v1::work_item_type::{
    v1_work_item_type_json, V1CreateWorkItemType, V1WorkItemTypeRow,
};

#[test]
fn work_item_type_json_has_sdk_required_and_optional_keys() {
    let row = v1_work_item_type_json(&V1WorkItemTypeRow {
        id: uuid::Uuid::nil(),
        name: "Bug".to_string(),
        description: Some("d".to_string()),
        logo_props: None,
        is_epic: false,
        is_default: false,
        is_active: true,
        level: Some(1),
        external_id: None,
        external_source: None,
        created_by: None,
        updated_by: None,
        workspace: uuid::Uuid::nil(),
        created_at: None,
        updated_at: None,
        deleted_at: None,
        project_ids: vec![],
    });
    assert_eq!(row["name"], serde_json::json!("Bug"));
    for key in [
        "id",
        "name",
        "description",
        "logo_props",
        "is_epic",
        "is_default",
        "is_active",
        "level",
        "external_id",
        "external_source",
        "created_by",
        "updated_by",
        "workspace",
        "created_at",
        "updated_at",
        "deleted_at",
        "project_ids",
    ] {
        assert!(row.get(key).is_some(), "missing WorkItemType key: {key}");
    }
    assert_eq!(row["project_ids"], serde_json::json!([]));
}

#[test]
fn create_body_accepts_missing_fields() {
    let body: V1CreateWorkItemType = serde_json::from_value(serde_json::json!({})).unwrap();
    assert!(body.name.is_none());
    assert!(body.project_ids.is_empty());
}
