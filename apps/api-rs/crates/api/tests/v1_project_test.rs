use api::routes::v1::project::{v1_project_features_json, v1_project_lite_json, ProjectLiteRow};
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

fn row() -> ProjectLiteRow {
    ProjectLiteRow {
        id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
        identifier: "PREPAID".to_string(),
        name: "Ne".to_string(),
        cover_image: Some("https://cdn/cover.png".to_string()),
        icon_prop: None,
        emoji: None,
        description: "d".to_string(),
        archived_at: None,
        cover_image_asset_id: None,
        cover_image_entity_type: None,
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

#[test]
fn project_lite_json_maps_sql_null_icon_prop_to_json_null() {
    let mut r = row();
    r.icon_prop = None;
    let v = v1_project_lite_json(&r);
    assert_eq!(v["icon_prop"], Value::Null);
}

#[test]
fn project_lite_json_cover_image_url_matches_helper_semantics() {
    // Asset-backed cover wins via the shared helper
    // (`/api/assets/v2/static/<id>/` for PROJECT_COVER).
    let asset_id = Uuid::parse_str("12345678-1234-5678-1234-567812345678").unwrap();
    let mut r = row();
    r.cover_image_asset_id = Some(asset_id);
    r.cover_image_entity_type = Some("PROJECT_COVER".to_string());
    r.cover_image = Some("https://x/y.png".to_string());
    assert_eq!(
        v1_project_lite_json(&r)["cover_image_url"],
        Value::String(format!("/api/assets/v2/static/{asset_id}/").into())
    );

    // No asset but legacy cover_image Some -> legacy value.
    let mut r = row();
    r.cover_image_asset_id = None;
    r.cover_image_entity_type = None;
    r.cover_image = Some("https://x/y.png".to_string());
    assert_eq!(
        v1_project_lite_json(&r)["cover_image_url"],
        Value::String("https://x/y.png".into())
    );

    // Both None -> null.
    let mut r = row();
    r.cover_image = None;
    r.cover_image_asset_id = None;
    r.cover_image_entity_type = None;
    assert_eq!(v1_project_lite_json(&r)["cover_image_url"], Value::Null);

    // Legacy "" counts as missing per helper semantics -> null.
    let mut r = row();
    r.cover_image = Some("".to_string());
    r.cover_image_asset_id = None;
    r.cover_image_entity_type = None;
    assert_eq!(v1_project_lite_json(&r)["cover_image_url"], Value::Null);
}

#[test]
fn project_features_json_maps_known_columns() {
    let v = v1_project_features_json(true, false, true, false, true, true);
    assert_eq!(v["modules"], Value::Bool(true));
    assert_eq!(v["cycles"], Value::Bool(false));
    assert_eq!(v["views"], Value::Bool(true));
    assert_eq!(v["pages"], Value::Bool(false));
    assert_eq!(v["intakes"], Value::Bool(true));
    assert_eq!(v["work_item_types"], Value::Bool(true));
}
