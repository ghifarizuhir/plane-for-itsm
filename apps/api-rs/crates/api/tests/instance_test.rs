use api::routes::instance::{env_flag, env_str_or, flag_enabled};

#[test]
fn flag_only_1_is_true() {
    assert!(flag_enabled("1"));
    assert!(!flag_enabled("0"));
    assert!(!flag_enabled(""));
    assert!(!flag_enabled("true"));
}

#[test]
fn env_flag_falls_back_to_default_when_missing() {
    // Use a key that must not exist in test env.
    assert!(env_flag("PLANE_RS_TEST_FLAG_MISSING_1_XYZ", "1"));
    assert!(!env_flag("PLANE_RS_TEST_FLAG_MISSING_1_XYZ", "0"));
}

#[test]
fn env_str_or_falls_back_to_default_when_missing() {
    assert_eq!(
        env_str_or("PLANE_RS_TEST_STR_MISSING_XYZ", "fallback"),
        "fallback"
    );
}
