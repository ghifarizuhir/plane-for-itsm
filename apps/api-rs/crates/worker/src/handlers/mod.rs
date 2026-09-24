pub mod ai_schedule;
pub mod cleanup;
pub mod email;
pub mod export;
pub mod file_asset;
pub mod issue_automation;
pub mod webhook;

use serde_json::Value;

pub async fn dispatch(name: &str, payload: Value) -> anyhow::Result<()> {
    match name {
        "email.notification" => email::handle(payload).await,
        "webhook.dispatch" => webhook::handle(payload).await,
        "cleanup.api_logs" => {
            anyhow::bail!("cleanup.api_logs requires pool — use dispatch_with_pool")
        }
        "issue.archive" => {
            anyhow::bail!("issue.archive requires pool — use dispatch_with_pool")
        }
        _ => anyhow::bail!("unknown job {}", name),
    }
}

pub async fn dispatch_with_pool(
    pool: &sqlx::PgPool,
    name: &str,
    payload: Value,
) -> anyhow::Result<()> {
    match name {
        "email.notification" => email::handle_with_pool(pool, payload).await,
        "webhook.dispatch" => webhook::handle(payload).await,
        "cleanup.api_logs" => cleanup::api_logs(pool).await,
        "cleanup.email_logs" => cleanup::email_notification_logs(pool).await,
        "cleanup.page_versions" => cleanup::page_versions(pool).await,
        "cleanup.issue_desc" => cleanup::issue_description_versions(pool).await,
        "cleanup.webhook_logs" => cleanup::webhook_logs(pool).await,
        "issue.archive" => issue_automation::archive(pool).await,
        "file_asset.delete_unuploaded" => file_asset::delete_unuploaded(pool).await,
        "export.delete_old_s3" => export::delete_old_s3(pool).await,
        _ => dispatch(name, payload).await,
    }
}

/// Stream jobs this build is allowed to execute. Everything else beat may
/// enqueue stays disabled until validated individually.
pub fn is_enabled_job(name: &str) -> bool {
    matches!(name, "ai.schedule.tick" | "ai.schedule.run")
}

/// Read one stream entry and dispatch the allowlisted AI schedule jobs.
/// Other beat jobs stay disabled until they are validated individually.
pub async fn handle_by_id(
    pool: &sqlx::PgPool,
    redis: &mut redis::aio::ConnectionManager,
    id: &str,
) -> anyhow::Result<()> {
    let Some((job, payload)) = common::stream::job_by_id(redis, id).await? else {
        tracing::warn!(id=%id, "stream entry vanished before dispatch");
        return Ok(());
    };
    if !is_enabled_job(&job) {
        tracing::warn!(job=%job, id=%id, "job disabled (not yet enabled)");
        return Ok(());
    }
    match job.as_str() {
        "ai.schedule.tick" => {
            let queued = ai_schedule::tick(pool, redis).await?;
            tracing::info!(queued=%queued.len(), "ai.schedule.tick done");
            Ok(())
        }
        "ai.schedule.run" => ai_schedule::run(pool, payload).await,
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ai_schedule_jobs_are_enabled() {
        assert!(is_enabled_job("ai.schedule.tick"));
        assert!(is_enabled_job("ai.schedule.run"));
        assert!(!is_enabled_job("email.notification"));
        assert!(!is_enabled_job("issue.archive"));
        assert!(!is_enabled_job(""));
    }
}
