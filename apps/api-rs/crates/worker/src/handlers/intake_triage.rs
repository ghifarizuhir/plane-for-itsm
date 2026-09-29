//! Jev intake triage jobs: classify one item, or sweep pending items.

use redis::aio::ConnectionManager;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn classify(pool: &PgPool, payload: Value) -> anyhow::Result<()> {
    let Some(intake_issue_id) = payload
        .get("intake_issue_id")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
    else {
        tracing::warn!(payload=%payload, "ai.intake.triage: missing intake_issue_id");
        return Ok(());
    };
    ai::triage_job::classify(pool, intake_issue_id).await?;
    Ok(())
}

pub async fn sweep(pool: &PgPool, redis: &mut ConnectionManager) -> anyhow::Result<()> {
    if ai::decision::resolve_decision_config(pool).await.is_none() {
        tracing::debug!("ai.intake.triage.sweep: decision model not configured");
        return Ok(());
    }
    let candidates = ai::triage_job::sweep_candidates(pool, ai::triage_job::SWEEP_LIMIT).await?;
    for intake_issue_id in &candidates {
        if let Err(error) = common::stream::push_job(
            redis,
            "ai.intake.triage",
            json!({"intake_issue_id": intake_issue_id}),
        )
        .await
        {
            tracing::error!(intake_issue_id=%intake_issue_id, error=%error, "ai.intake.triage.sweep: push failed");
        }
    }
    tracing::info!(queued=%candidates.len(), "ai.intake.triage.sweep done");
    Ok(())
}
