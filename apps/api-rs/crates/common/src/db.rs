use sqlx::{postgres::PgPoolOptions, PgPool};

use crate::config::AppConfig;

pub async fn create_pool(cfg: &AppConfig) -> PgPool {
    PgPoolOptions::new()
        .max_connections(5)
        .min_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&cfg.database_url)
        .await
        .expect("pg connect failed")
}

pub async fn ping(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}

/// Apply embedded sqlx migrations (idempotent baseline + deltas).
pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("../../migrations").run(pool).await?;
    Ok(())
}
