use api::routes::misc::{
    api_token_created_json, api_token_read_json, default_token_label, token_is_active, validate_export_provider,
};
use chrono::{TimeZone, Utc};
use common::models::misc::ApiToken;
use serde_json::Value;
use uuid::Uuid;

#[test]
fn rejects_unknown_export_provider() {
    let err = validate_export_provider(Some("xml")).unwrap_err();
    assert!(err.contains("Provider 'xml' not found."));
}

#[test]
fn rejects_missing_export_provider() {
    assert!(validate_export_provider(None).is_err());
}

#[test]
fn accepts_valid_export_providers() {
    for p in ["csv", "xlsx", "json"] {
        assert!(validate_export_provider(Some(p)).is_ok());
    }
}

#[test]
fn token_label_defaults_to_hex() {
    let label = default_token_label(None);
    assert_eq!(label.len(), 32);
    assert!(label.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(default_token_label(Some("ci".to_string())), "ci");
}

fn sample_token(description: &str, expired_at: Option<chrono::DateTime<Utc>>) -> ApiToken {
    let ts = Utc.with_ymd_and_hms(2026, 9, 18, 8, 0, 0).unwrap();
    ApiToken {
        id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
        label: "ci".to_string(),
        description: description.to_string(),
        last_used: None,
        user_type: 0,
        created_by_id: None,
        updated_by_id: None,
        user_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
        workspace_id: None,
        expired_at,
        is_service: false,
        allowed_rate_limit: "60/min".to_string(),
        created_at: ts,
        updated_at: ts,
        deleted_at: None,
    }
}

#[test]
fn token_is_active_handles_never_and_boundaries() {
    let now = Utc.with_ymd_and_hms(2026, 9, 18, 8, 0, 0).unwrap();
    assert!(token_is_active(None, now), "null expiry never expires");
    assert!(token_is_active(Some(now + chrono::Duration::seconds(1)), now));
    assert!(!token_is_active(Some(now - chrono::Duration::seconds(1)), now));
}

#[test]
fn read_json_matches_django_read_serializer_shape() {
    // Django `APITokenReadSerializer` excludes only `token`; the FE list page
    // dereferences `description`/`is_active`/`expired_at`, so the Rust port must
    // ship the same keys or the page crashes (regression: token.description.trim).
    let now = Utc.with_ymd_and_hms(2026, 9, 18, 8, 0, 0).unwrap();
    let token = sample_token("deploy bot", Some(now + chrono::Duration::days(30)));
    let value = api_token_read_json(&token, now);
    let obj = value.as_object().expect("object");
    for key in [
        "id",
        "label",
        "description",
        "is_active",
        "last_used",
        "user_type",
        "created_by",
        "updated_by",
        "user",
        "workspace",
        "expired_at",
        "is_service",
        "allowed_rate_limit",
        "created_at",
        "updated_at",
        "deleted_at",
    ] {
        assert!(obj.contains_key(key), "missing key {key}");
    }
    assert!(!obj.contains_key("token"), "read shape must not leak the secret");
    assert_eq!(obj["description"], Value::String("deploy bot".into()));
    assert_eq!(obj["is_active"], Value::Bool(true));
}

#[test]
fn read_json_keeps_empty_description_as_string() {
    // The FE calls `token.description.trim()`, so an omitted/null description
    // throws `can't access property "trim"`. Empty descriptions must be "".
    let now = Utc::now();
    let value = api_token_read_json(&sample_token("", None), now);
    assert_eq!(value["description"], Value::String(String::new()));
}

#[test]
fn created_json_includes_secret_token() {
    let now = Utc::now();
    let value = api_token_created_json(&sample_token("", None), "plane_api_abc", now);
    assert_eq!(value["token"], Value::String("plane_api_abc".into()));
    assert!(value.get("description").is_some());
}

#[tokio::test]
async fn timezones_envelope_with_offsets_sorted() {
    // Django `TimezoneEndpoint.get` (`timezone/base.py:184-215`):
    // `{"timezones": [{utc_offset, gmt_offset, value, label}]}` sorted by
    // (offset, label); offsets are DST-aware live values.
    let data = api::routes::misc::timezones().await.0;
    let arr = data.get("timezones").and_then(|v| v.as_array()).expect("envelope");
    assert!(arr.len() > 100, "codegen list intact: {}", arr.len());
    let mut prev: Option<(i32, String)> = None;
    for tz in arr {
        for k in ["utc_offset", "gmt_offset", "value", "label"] {
            assert!(tz.get(k).is_some(), "missing key {k}");
        }
        let utc = tz["utc_offset"].as_str().unwrap();
        let gmt = tz["gmt_offset"].as_str().unwrap();
        assert!(utc.starts_with("UTC") && gmt.starts_with("GMT"));
        assert_eq!(&utc[3..], &gmt[3..], "same stamp");
        // Parse "+HH:MM" back to seconds for the sort check.
        let s: Vec<char> = utc[3..].chars().collect();
        let secs: i32 = {
            let h: i32 = s[1..3].iter().collect::<String>().parse().unwrap();
            let m: i32 = s[4..6].iter().collect::<String>().parse().unwrap();
            (h * 3600 + m * 60) * if s[0] == '-' { -1 } else { 1 }
        };
        let label = tz["label"].as_str().unwrap().to_string();
        if let Some((po, pl)) = prev {
            assert!((secs, label.clone()) >= (po, pl), "sorted by (offset,label)");
        }
        prev = Some((secs, label));
    }
}
