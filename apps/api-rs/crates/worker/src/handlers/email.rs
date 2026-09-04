use serde_json::Value;

pub async fn handle(payload: Value) -> anyhow::Result<()> {
    tracing::info!(payload=?payload, "email.notification handled");
    Ok(())
}

pub async fn handle_with_pool(pool: &sqlx::PgPool, payload: Value) -> anyhow::Result<()> {
    sqlx::query("SELECT 1").execute(pool).await?;
    handle(payload).await
}
