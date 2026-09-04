use api::routes::notification::{validate_preference_patch, PREFERENCE_KEYS};
use std::collections::HashMap;

#[test]
fn rejects_unknown_preference_keys() {
    let mut patch = HashMap::new();
    patch.insert("telepathy".to_string(), serde_json::json!(true));
    let err = validate_preference_patch(&patch).unwrap_err();
    assert!(err.to_lowercase().contains("unknown"));
}

#[test]
fn accepts_known_preference_keys() {
    for key in PREFERENCE_KEYS {
        let mut patch = HashMap::new();
        patch.insert(key.to_string(), serde_json::json!(false));
        assert!(validate_preference_patch(&patch).is_ok(), "key={key}");
    }
}

#[test]
fn rejects_non_bool_preference_value() {
    let mut patch = HashMap::new();
    patch.insert("comment".to_string(), serde_json::json!("yes"));
    let err = validate_preference_patch(&patch).unwrap_err();
    assert!(err.to_lowercase().contains("boolean"));
}
