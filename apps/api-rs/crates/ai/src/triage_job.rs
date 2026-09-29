//! DB-backed triage jobs shared by the worker: claim a pending intake item,
//! ask Jev, persist the suggestion; list sweep candidates.

use sqlx::PgPool;
use uuid::Uuid;

use crate::decision::{self, DecisionError};
use crate::triage::{self, TypeOption};

pub const MAX_ATTEMPTS: i32 = 3;
pub const SWEEP_LIMIT: i64 = 10;
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(sqlx::FromRow)]
struct PendingItem {
    intake_issue_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    name: String,
    description: Option<String>,
    source: Option<String>,
    project_name: String,
}

async fn load_item(pool: &PgPool, intake_issue_id: Uuid) -> Result<Option<PendingItem>, sqlx::Error> {
    sqlx::query_as(
        "SELECT ii.id AS intake_issue_id, ii.project_id, ii.workspace_id, \
                i.name, i.description_stripped AS description, ii.source, p.name AS project_name \
         FROM intake_issues ii \
         JOIN issues i ON i.id = ii.issue_id \
         JOIN projects p ON p.id = ii.project_id \
         WHERE ii.id = $1 AND ii.deleted_at IS NULL AND ii.status = -2 AND i.deleted_at IS NULL",
    )
    .bind(intake_issue_id)
    .fetch_optional(pool)
    .await
}

async fn load_types(pool: &PgPool, project_id: Uuid) -> Result<Vec<TypeOption>, sqlx::Error> {
    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT t.id, t.name, t.description FROM project_issue_types pit \
         JOIN issue_types t ON t.id = pit.issue_type_id \
         WHERE pit.project_id = $1 AND pit.deleted_at IS NULL \
           AND t.deleted_at IS NULL AND t.is_epic = false \
         ORDER BY t.name LIMIT $2",
    )
    .bind(project_id)
    .bind(triage::MAX_TYPES as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(type_id, name, description)| TypeOption { type_id, name, description })
        .collect())
}

/// Insert a pending row; on conflict take over a failed row below the attempt
/// cap. `true` means this invocation owns the item.
async fn claim(
    pool: &PgPool,
    intake_issue_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let inserted: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO intake_triage_suggestions \
            (id, intake_issue_id, project_id, workspace_id, status, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, 'pending', now(), now()) \
         ON CONFLICT (intake_issue_id) DO NOTHING RETURNING id",
    )
    .bind(intake_issue_id)
    .bind(project_id)
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;
    if inserted.is_some() {
        return Ok(true);
    }
    let retried: Option<Uuid> = sqlx::query_scalar(
        "UPDATE intake_triage_suggestions SET status = 'pending', updated_at = now() \
         WHERE intake_issue_id = $1 AND status = 'failed' AND attempts < $2 RETURNING id",
    )
    .bind(intake_issue_id)
    .bind(MAX_ATTEMPTS)
    .fetch_optional(pool)
    .await?;
    Ok(retried.is_some())
}

async fn save_ready(
    pool: &PgPool,
    intake_issue_id: Uuid,
    mapped: &triage::TriageOutcome,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE intake_triage_suggestions SET \
            status = 'ready', model = $2, answers = $3, \
            category_type_id = $4, category_label = $5, category_confidence = $6, \
            severity_priority = $7, severity_score = $8, severity_confidence = $9, \
            needs_human = $10, input_tokens = $11, output_tokens = $12, \
            last_error = NULL, updated_at = now() \
         WHERE intake_issue_id = $1",
    )
    .bind(intake_issue_id)
    .bind(&mapped.model)
    .bind(&mapped.answers)
    .bind(mapped.category_type_id)
    .bind(&mapped.category_label)
    .bind(mapped.category_confidence)
    .bind(&mapped.severity_priority)
    .bind(mapped.severity_score)
    .bind(mapped.severity_confidence)
    .bind(mapped.needs_human)
    .bind(mapped.input_tokens as i32)
    .bind(mapped.output_tokens as i32)
    .execute(pool)
    .await?;
    Ok(())
}

async fn save_failed(pool: &PgPool, intake_issue_id: Uuid, message: &str) -> Result<(), sqlx::Error> {
    let truncated: String = message.chars().take(500).collect();
    sqlx::query(
        "UPDATE intake_triage_suggestions SET status = 'failed', attempts = attempts + 1, \
         last_error = $2, updated_at = now() WHERE intake_issue_id = $1",
    )
    .bind(intake_issue_id)
    .bind(truncated)
    .execute(pool)
    .await?;
    Ok(())
}

fn decision_error_message(error: DecisionError) -> &'static str {
    match error {
        DecisionError::NotConfigured => "decision model is not configured",
        DecisionError::RateLimited => "decision model rate limited",
        DecisionError::InvalidRequest => "decision request was rejected (422)",
        DecisionError::Upstream => "decision upstream error",
        DecisionError::Timeout => "decision request timed out",
    }
}

/// Classify one intake item. No configuration → no row (sweep retries later);
/// already claimed/ready → no-op; decision failures are stored, not returned.
pub async fn classify(pool: &PgPool, intake_issue_id: Uuid) -> Result<(), sqlx::Error> {
    let Some(config) = decision::resolve_decision_config(pool).await else {
        tracing::debug!(intake_issue_id=%intake_issue_id, "ai.intake.triage: decision model not configured");
        return Ok(());
    };
    let Some(item) = load_item(pool, intake_issue_id).await? else {
        tracing::debug!(intake_issue_id=%intake_issue_id, "ai.intake.triage: item not pending");
        return Ok(());
    };
    if !claim(pool, item.intake_issue_id, item.project_id, item.workspace_id).await? {
        tracing::debug!(intake_issue_id=%intake_issue_id, "ai.intake.triage: already claimed");
        return Ok(());
    }
    let types = load_types(pool, item.project_id).await?;
    let state = triage::build_state(triage::TriageState {
        name: &item.name,
        description: item.description.as_deref().unwrap_or(""),
        project_name: &item.project_name,
        source: item.source.as_deref().unwrap_or("IN_APP"),
    });
    let questions = decision::questions_json(&triage::build_questions(&types));
    match tokio::time::timeout(REQUEST_TIMEOUT, decision::ask(&config, &state, questions)).await {
        Ok(Ok(outcome)) => {
            let mapped = triage::triage_outcome(outcome, &types);
            save_ready(pool, item.intake_issue_id, &mapped).await?;
        }
        Ok(Err(error)) => {
            tracing::warn!(intake_issue_id=%intake_issue_id, error=?error, "ai.intake.triage: decision failed");
            save_failed(pool, item.intake_issue_id, decision_error_message(error)).await?;
        }
        Err(_) => {
            save_failed(pool, item.intake_issue_id, "decision request timed out").await?;
        }
    }
    Ok(())
}

/// Pending intake items that still need a suggestion: no row at all, or a
/// failed row past the 5-minute cooldown with attempts left.
pub async fn sweep_candidates(pool: &PgPool, limit: i64) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT ii.id FROM intake_issues ii \
         LEFT JOIN intake_triage_suggestions s ON s.intake_issue_id = ii.id \
         WHERE ii.status = -2 AND ii.deleted_at IS NULL \
           AND (s.id IS NULL OR (s.status = 'failed' AND s.attempts < $2 \
                AND s.updated_at < now() - interval '5 minutes')) \
         ORDER BY ii.created_at ASC LIMIT $1",
    )
    .bind(limit)
    .bind(MAX_ATTEMPTS)
    .fetch_all(pool)
    .await
}
