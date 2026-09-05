// crates/common/tests/jwt_cookie_test.rs
#[test]
fn access_roundtrip() {
    let uid = uuid::Uuid::new_v4();
    let tok = common::auth::encode_access(&uid, "s3cr3t", 900);
    let got = common::auth::decode_access(&tok, "s3cr3t").expect("valid");
    assert_eq!(got, uid);
}

#[test]
fn wrong_secret_rejected() {
    let uid = uuid::Uuid::new_v4();
    let tok = common::auth::encode_access(&uid, "s3cr3t", 900);
    assert!(common::auth::decode_access(&tok, "lain").is_err());
}

#[test]
fn cookie_headers_shape() {
    let dev = common::auth::cookie_headers("plane_at", "abc", 900, false);
    assert!(dev.contains("plane_at=abc"));
    assert!(dev.contains("HttpOnly"));
    assert!(dev.contains("SameSite=Lax"));
    assert!(!dev.contains("Secure"));
    let prod = common::auth::cookie_headers("__Host-plane_at", "abc", 900, true);
    assert!(prod.contains("Secure"));
}

#[test]
fn clear_cookie_expires_immediately() {
    let h = common::auth::clear_cookie_header("plane_at", false);
    assert!(h.contains("Max-Age=0"));
}
