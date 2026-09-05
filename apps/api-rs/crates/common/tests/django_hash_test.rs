const VECTOR: &str = "pbkdf2_sha256$1000000$smokesalt12345678$NyV386oOw4JpTzGNeUTSCrrCWBd/LOQxHJY3lHfakhI=";

#[test]
fn accepts_correct_password() {
    assert!(common::auth::verify_django_password("plan-vector-ok", VECTOR));
}

#[test]
fn rejects_wrong_password() {
    assert!(!common::auth::verify_django_password("salah", VECTOR));
}

#[test]
fn rejects_unknown_format() {
    assert!(!common::auth::verify_django_password("x", "bcrypt$abc"));
    assert!(!common::auth::verify_django_password("x", ""));
}

#[test]
fn rejects_malformed_digests() {
    // Empty digest must never verify (regression: empty vec fold == 0 was true).
    assert!(!common::auth::verify_django_password(
        "anything",
        "pbkdf2_sha256$1000000$salt$"
    ));
    // Non-numeric iteration count.
    assert!(!common::auth::verify_django_password(
        "x",
        "pbkdf2_sha256$notanumber$salt$QUJD"
    ));
    // Invalid base64 digest.
    assert!(!common::auth::verify_django_password(
        "x",
        "pbkdf2_sha256$1000000$salt$!!!not-base64!!!"
    ));
    // Trailing extra field.
    assert!(!common::auth::verify_django_password(
        "x",
        "pbkdf2_sha256$1000000$salt$QUJD$extra"
    ));
}

#[test]
fn make_then_verify_roundtrip() {
    let h = common::auth::make_django_password("rahasia-baru");
    assert!(h.starts_with("pbkdf2_sha256$1000000$"));
    assert!(common::auth::verify_django_password("rahasia-baru", &h));
}
