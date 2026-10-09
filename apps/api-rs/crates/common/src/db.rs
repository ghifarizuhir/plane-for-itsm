use sqlx::{postgres::{PgConnection, PgPoolOptions}, PgPool};

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
pub async fn migrate(database_url: &str) -> anyhow::Result<()> {
    migrate_with_statement_timeout(database_url, "60s").await
}

pub async fn migrate_with_statement_timeout(database_url: &str, statement_timeout: &str) -> anyhow::Result<()> {
    let timeout = statement_timeout.to_string();
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |conn: &mut PgConnection, _| {
            let timeout = timeout.clone();
            Box::pin(async move {
                // `statement_timeout` bounds the advisory-lock wait below (a
                // waiting `SELECT pg_advisory_lock` is cancelled on timeout).
                // Internal constant only, never user input.
                sqlx::query(&format!("SET statement_timeout = '{timeout}'"))
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await?;
    let result = sqlx::migrate!("../../migrations").run(&pool).await;
    // Always close: dropping the session releases any advisory lock that
    // `Migrator::run` leaves held on its error paths (it only unlocks on
    // full success), so a failed run can never wedge the next migrator.
    pool.close().await;
    result?;
    Ok(())
}
