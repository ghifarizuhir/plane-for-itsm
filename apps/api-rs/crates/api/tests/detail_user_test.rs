use api::routes::user::validate_new_email;

#[test]
fn new_email_requires_value() {
    let err = validate_new_email("").unwrap_err();
    assert!(err.contains("Email is required"));
}

#[test]
fn new_email_rejects_bad_format() {
    assert!(validate_new_email("not-an-email").is_err());
    assert!(validate_new_email("a@b").is_err());
    assert!(validate_new_email("@x.com").is_err());
}

#[test]
fn new_email_accepts_valid() {
    assert!(validate_new_email("user@example.com").is_ok());
}
