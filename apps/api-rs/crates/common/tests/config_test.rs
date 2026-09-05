#[test]
fn config_from_env_defaults() {
    // clear env to test defaults
    std::env::remove_var("DATABASE_URL");
    std::env::remove_var("REDIS_URL");
    std::env::remove_var("PORT");
    let cfg = common::config::AppConfig::from_env();
    assert!(cfg.database_url.contains("postgres"));
    assert!(cfg.redis_url.contains("redis"));
    assert_eq!(cfg.port, 8001);
}

#[test]
fn cookie_secure_defaults_off() {
    std::env::remove_var("COOKIE_SECURE");
    std::env::remove_var("FRONTEND_URL");
    let cfg = common::config::AppConfig::from_env();
    assert!(!cfg.cookie_secure);
    assert_eq!(cfg.frontend_url, "http://localhost:3000");
}
