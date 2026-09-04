use api::routes::page::{validate_create, CreatePage};

#[test]
fn accepts_blank_name() {
    // Django: name = TextField(blank=True) — untitled pages allowed
    let p = CreatePage {
        name: None,
        access: None,
        color: None,
    };
    assert!(validate_create(&p).is_ok());
}

#[test]
fn rejects_color_over_255() {
    let p = CreatePage {
        name: Some("Doc".to_string()),
        access: None,
        color: Some("x".repeat(256)),
    };
    let err = validate_create(&p).unwrap_err();
    assert!(err.to_lowercase().contains("255"));
}

#[test]
fn rejects_invalid_access() {
    let p = CreatePage {
        name: Some("Doc".to_string()),
        access: Some(9),
        color: None,
    };
    let err = validate_create(&p).unwrap_err();
    assert!(err.to_lowercase().contains("access"));
}

#[test]
fn accepts_valid_page() {
    for a in [None, Some(0), Some(1)] {
        let p = CreatePage {
            name: Some("Doc".to_string()),
            access: a,
            color: None,
        };
        assert!(validate_create(&p).is_ok());
    }
}
