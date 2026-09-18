use api::routes::v1::project::{v1_project_lite_json, ProjectLiteRow};
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

fn row() -> ProjectLiteRow {
    ProjectLiteRow {
        id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
        identifier: "PREPAID".to_string(),
        name: "Ne".to_string(),
        cover_image: None,
        icon_prop: Value::Null,
        emoji: None,
        description: "d".to_string(),
        archived_at: None,
        cover_asset: Some("https://cdn/cover.png".to_string()),
    }
}

#[test]
fn project_lite_json_has_sdk_required_fields_and_keys() {
    let v = v1_project_lite_json(&row());
    let o = v.as_object().expect("object");
    for key in ["id", "identifier", "name", "cover_image", "icon_prop", "emoji", "description", "cover_image_url", "archived_at"] {
        assert!(o.contains_key(key), "missing key {key}");
    }
    assert_eq!(o["identifier"], Value::String("PREPAID".into()));
    assert_eq!(o["name"], Value::String("Ne".into()));
    assert_eq!(o["cover_image_url"], Value::String("https://cdn/cover.png".into()));
    let _ = Utc::now();
}
