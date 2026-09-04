pub async fn delete_old_s3(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM exporter WHERE created_at < now() - interval '1 day'")
        .execute(pool)
        .await?;
    Ok(())
}
