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
