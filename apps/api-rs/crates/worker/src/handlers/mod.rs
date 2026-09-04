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

pub async fn handle_by_id(
    pool: &sqlx::PgPool,
    _redis: &mut redis::aio::ConnectionManager,
    id: &str,
) -> anyhow::Result<()> {
    tracing::info!(id=%id, "handle_by_id stub");
    let _ = pool;
    Ok(())
}
