use api::routes::view::{validate_create, validate_favorite_create, CreateView, CreateFavorite};

#[test]
fn rejects_empty_name() {
    let v = CreateView {
        name: "".to_string(),
        description: None,
        access: None,
    };
    let err = validate_create(&v).unwrap_err();
    assert!(err.to_lowercase().contains("name"));
}

#[test]
fn rejects_name_over_255() {
    let v = CreateView {
        name: "x".repeat(256),
        description: None,
        access: None,
    };
    let err = validate_create(&v).unwrap_err();
    assert!(err.to_lowercase().contains("255"));
}

#[test]
fn rejects_invalid_access() {
    let v = CreateView {
        name: "Mine".to_string(),
        description: None,
        access: Some(7),
    };
    let err = validate_create(&v).unwrap_err();
    assert!(err.to_lowercase().contains("access"));
}

#[test]
fn accepts_valid_view() {
    for a in [None, Some(0), Some(1)] {
        let v = CreateView {
            name: "Mine".to_string(),
            description: None,
            access: a,
        };
        assert!(validate_create(&v).is_ok());
    }
}

#[test]
fn favorite_rejects_missing_view() {
    let f = CreateFavorite { view: None };
    let err = validate_favorite_create(&f).unwrap_err();
    assert!(err.to_lowercase().contains("view"));
}

#[test]
fn favorite_accepts_valid() {
    let f = CreateFavorite {
        view: Some(uuid::Uuid::new_v4()),
    };
    assert!(validate_favorite_create(&f).is_ok());
}
