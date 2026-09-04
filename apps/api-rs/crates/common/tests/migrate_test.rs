//! Plan 3.1 Step 2: migrate helper must apply the idempotent baseline.

#[tokio::test]
async fn migrate_runs() {
    let pool = common::db::create_pool(&common::config::AppConfig::from_env()).await;
    common::db::migrate(&pool).await.unwrap();
    // Baseline applied: core tables exist.
    let (exists,): (bool,) =
        sqlx::query_as("SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name='workspaces')")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(exists, "workspaces table must exist after migrate");
    // Idempotency: forget the tracked version and re-apply raw SQL guards.
    sqlx::query("DELETE FROM _sqlx_migrations")
        .execute(&pool)
        .await
        .unwrap();
    common::db::migrate(&pool).await.unwrap();
}
