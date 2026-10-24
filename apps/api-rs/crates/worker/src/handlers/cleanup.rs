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
    sqlx::query(
        "DELETE FROM issue_description_version WHERE created_at < now() - interval '30 days'",
    )
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

pub fn retention_interval(resolution: i32) -> Option<&'static str> {
    match resolution {
        60 => Some("48 hours"),
        300 => Some("14 days"),
        600 => Some("30 days"),
        1800 => Some("90 days"),
        3600 => Some("365 days"),
        _ => None,
    }
}

pub async fn service_metric_buckets(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    for resolution in [60_i32, 300, 600, 1800, 3600] {
        let Some(interval) = retention_interval(resolution) else {
            continue;
        };
        let mut total = 0_u64;
        loop {
            let result = sqlx::query(
                "DELETE FROM service_metric_buckets WHERE id IN ( \
                   SELECT id FROM service_metric_buckets \
                   WHERE resolution = $1 AND bucket_start < now() - $2::interval \
                   LIMIT 10000)",
            )
            .bind(i16::try_from(resolution).unwrap_or(60))
            .bind(interval)
            .execute(pool)
            .await?;
            let deleted = result.rows_affected();
            total += deleted;
            if deleted < 10_000 {
                break;
            }
        }
        tracing::info!(resolution, deleted = total, "service metric buckets cleaned");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::retention_interval;

    #[test]
    fn retention_intervals_cover_every_resolution() {
        assert_eq!(retention_interval(60), Some("48 hours"));
        assert_eq!(retention_interval(300), Some("14 days"));
        assert_eq!(retention_interval(600), Some("30 days"));
        assert_eq!(retention_interval(1800), Some("90 days"));
        assert_eq!(retention_interval(3600), Some("365 days"));
        assert_eq!(retention_interval(120), None);
    }
}
