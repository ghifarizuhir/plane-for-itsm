use api::routes::label::{validate_create, CreateLabel};

fn minimal(name: &str) -> CreateLabel {
    CreateLabel {
        name: name.to_string(),
        color: None,
        description: None,
        external_source: None,
        external_id: None,
        parent_id: None,
        sort_order: None,
    }
}

#[test]
fn rejects_empty_name() {
    let err = validate_create(&minimal("")).unwrap_err();
    assert!(err.to_lowercase().contains("name"));
}

#[test]
fn rejects_blank_name() {
    let err = validate_create(&minimal("   ")).unwrap_err();
    assert!(err.to_lowercase().contains("name"));
}

#[test]
fn rejects_name_over_255() {
    let err = validate_create(&minimal(&"n".repeat(256))).unwrap_err();
    assert!(err.contains("255"));
}

#[test]
fn rejects_color_over_255() {
    let mut body = minimal("Bug");
    body.color = Some("c".repeat(256));
    let err = validate_create(&body).unwrap_err();
    assert!(err.to_lowercase().contains("color"));
}

#[test]
fn accepts_minimal() {
    assert!(validate_create(&minimal("Bug")).is_ok());
}

#[test]
fn accepts_full_payload() {
    let body = CreateLabel {
        name: "Frontend".to_string(),
        color: Some("#ff0000".to_string()),
        description: Some("UI work".to_string()),
        external_source: Some("github".to_string()),
        external_id: Some("123".to_string()),
        parent_id: Some(uuid::Uuid::new_v4()),
        sort_order: Some(65535.0),
    };
    assert!(validate_create(&body).is_ok());
}
