use api::routes::misc::{validate_export_provider, default_token_label};

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
