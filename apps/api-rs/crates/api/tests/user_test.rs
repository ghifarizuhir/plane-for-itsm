use api::routes::user::{validate_update, UpdateUser};

#[test]
fn rejects_first_name_with_url() {
    let u = UpdateUser {
        first_name: Some("Hi http://example.com".to_string()),
        last_name: None,
    };
    let err = validate_update(&u).unwrap_err();
    assert!(err.to_lowercase().contains("url"));
}

#[test]
fn rejects_last_name_with_url() {
    let u = UpdateUser {
        first_name: None,
        last_name: Some("www.example.com".to_string()),
    };
    assert!(validate_update(&u).is_err());
}

#[test]
fn accepts_valid_names() {
    let u = UpdateUser {
        first_name: Some("Ada".to_string()),
        last_name: Some("Lovelace".to_string()),
    };
    assert!(validate_update(&u).is_ok());
}

#[test]
fn accepts_empty_update() {
    let u = UpdateUser {
        first_name: None,
        last_name: None,
    };
    assert!(validate_update(&u).is_ok());
}
