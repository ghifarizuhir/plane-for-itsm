pub async fn archive(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE issue SET archived_at=now() WHERE state_id IN (SELECT id FROM state WHERE group_name='completed') AND updated_at < now() - interval '30 days' AND archived_at IS NULL",
    )
    .execute(pool)
    .await?;
    Ok(())
}
