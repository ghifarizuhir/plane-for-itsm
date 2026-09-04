pub async fn delete_unuploaded(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM file_asset WHERE is_uploaded=false AND created_at < now() - interval '1 day'")
        .execute(pool)
        .await?;
    Ok(())
}
