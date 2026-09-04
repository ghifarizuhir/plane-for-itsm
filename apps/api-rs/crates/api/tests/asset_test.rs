use api::routes::asset::{validate_upload_init, clamp_size, CreateAssetInit, FILE_SIZE_LIMIT};

#[test]
fn rejects_invalid_entity_type() {
    let a = CreateAssetInit {
        name: Some("a.png".to_string()),
        file_type: Some("image/png".to_string()),
        size: Some(100),
        entity_type: "SPACESHIP".to_string(),
        entity_identifier: None,
    };
    let err = validate_upload_init(&a).unwrap_err();
    assert!(err.to_lowercase().contains("invalid entity type"));
}

#[test]
fn rejects_invalid_file_type() {
    let a = CreateAssetInit {
        name: Some("a.exe".to_string()),
        file_type: Some("application/x-msdownload".to_string()),
        size: Some(100),
        entity_type: "ISSUE_ATTACHMENT".to_string(),
        entity_identifier: None,
    };
    let err = validate_upload_init(&a).unwrap_err();
    assert!(err.to_lowercase().contains("invalid file type"));
}

#[test]
fn accepts_valid_init() {
    for t in ["image/jpeg", "image/png", "image/webp", "image/jpg", "image/gif"] {
        let a = CreateAssetInit {
            name: Some("a".to_string()),
            file_type: Some(t.to_string()),
            size: Some(100),
            entity_type: "ISSUE_ATTACHMENT".to_string(),
            entity_identifier: None,
        };
        assert!(validate_upload_init(&a).is_ok());
    }
}

#[test]
fn clamps_size_to_limit() {
    assert_eq!(clamp_size(FILE_SIZE_LIMIT as i64 * 10), FILE_SIZE_LIMIT);
    assert_eq!(clamp_size(100), 100);
}
