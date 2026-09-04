pub async fn api_logs(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM api_logs WHERE created_at < now() - interval '30 days'")
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn email_notification_logs(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM email_notification_log WHERE created_at < now() - interval '30 days'")
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn page_versions(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM page_version WHERE created_at < now() - interval '30 days'")
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn issue_description_versions(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM issue_description_version WHERE created_at < now() - interval '30 days'")
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn webhook_logs(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM webhook_log WHERE created_at < now() - interval '30 days'")
        .execute(pool)
        .await?;
    Ok(())
}
