//! AI schedule jobs: claim due schedules (`tick`) and run the agent (`run`).

use ai::schedule::ScheduleProposal;
use redis::aio::ConnectionManager;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
struct DueSchedule {
    id: Uuid,
    prompt: String,
    frequency: String,
    time_of_day: String,
    day_of_week: Option<i16>,
    day_of_month: Option<i16>,
    timezone: String,
}

impl DueSchedule {
    fn proposal(&self) -> Result<ScheduleProposal, String> {
        ScheduleProposal::new(
            "scheduled",
            &self.prompt,
            &self.frequency,
            Some(&self.time_of_day),
            self.day_of_week,
            self.day_of_month,
            Some(&self.timezone),
        )
    }
}

/// Tools a run may use: the validated spec subset, or every read tool for
/// legacy rows (`spec IS NULL`).
fn allowed_tools(spec: Option<Value>) -> Result<Vec<String>, String> {
    match spec {
        Some(value) => serde_json::from_value::<ai::schedule::ScheduleSpec>(value)
            .map_err(|error| error.to_string())
            .and_then(|spec| spec.validated())
            .map(|spec| spec.tools),
        None => Ok(ai::schedule::SPEC_TOOLS
            .iter()
            .map(|name| name.to_string())
            .collect()),
    }
}

/// Mark runs stuck in queued/running as failed. `running` uses `started_at`
/// (agent timeout is 180 s, so 15 minutes means genuinely stuck); `queued`
/// uses a horizon larger than the worst-case serial drain of one tick batch.
async fn sweep_stuck_runs(pool: &PgPool) -> anyhow::Result<()> {
    let swept_running: Vec<Uuid> = sqlx::query_scalar(
        "UPDATE ai_schedule_runs SET status = 'failed', \
         error = 'run did not finish within 15 minutes', finished_at = now() \
         WHERE status = 'running' AND started_at < now() - interval '15 minutes' \
         RETURNING id",
    )
    .fetch_all(pool)
    .await?;
    let swept_queued: Vec<Uuid> = sqlx::query_scalar(
        "UPDATE ai_schedule_runs SET status = 'failed', \
         error = 'run was never started (worker backlog or lost job)', finished_at = now() \
         WHERE status = 'queued' AND created_at < now() - interval '6 hours' \
         RETURNING id",
    )
    .fetch_all(pool)
    .await?;
    for run_id in swept_running.into_iter().chain(swept_queued) {
        insert_run_notification(pool, run_id).await?;
    }
    Ok(())
}

/// Claim due schedules (row-locked, atomic) and queue one run per schedule.
/// Returns the queued run ids (empty when nothing was due).
pub async fn tick(pool: &PgPool, redis: &mut ConnectionManager) -> anyhow::Result<Vec<Uuid>> {
    sweep_stuck_runs(pool).await?;
    let mut tx = pool.begin().await?;
    let db_now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT now()")
        .fetch_one(&mut *tx)
        .await?;
    let due: Vec<DueSchedule> = sqlx::query_as(
        "SELECT id, prompt, frequency, time_of_day, day_of_week, day_of_month, timezone \
         FROM ai_schedules \
         WHERE enabled = true AND deleted_at IS NULL AND next_run_at <= now() \
         ORDER BY next_run_at LIMIT 50 FOR UPDATE SKIP LOCKED",
    )
    .fetch_all(&mut *tx)
    .await?;

    let mut queued = Vec::new();
    for schedule in &due {
        let Ok(proposal) = schedule.proposal() else {
            // DB CHECKs cover most preset fields, but prompt/timezone validity
            // still lives in Rust; pause the row so it cannot slide forever.
            tracing::error!(schedule_id=%schedule.id, "ai.schedule.tick: invalid preset, disabling schedule");
            sqlx::query(
                "UPDATE ai_schedules SET enabled = false, updated_at = now() WHERE id = $1",
            )
            .bind(schedule.id)
            .execute(&mut *tx)
            .await?;
            continue;
        };
        let next = proposal.next_occurrence(db_now);
        sqlx::query("UPDATE ai_schedules SET next_run_at = $2, updated_at = now() WHERE id = $1")
            .bind(schedule.id)
            .bind(next)
            .execute(&mut *tx)
            .await?;
        let run_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO ai_schedule_runs (id, schedule_id, workspace_id, status, trigger, prompt, created_at) \
             SELECT $1, s.id, s.workspace_id, 'queued', 'scheduled', $3, now() \
             FROM ai_schedules s WHERE s.id = $2",
        )
        .bind(run_id)
        .bind(schedule.id)
        .bind(&schedule.prompt)
        .execute(&mut *tx)
        .await?;
        queued.push(run_id);
    }
    tx.commit().await?;

    let mut push_error: Option<anyhow::Error> = None;
    for run_id in &queued {
        if let Err(error) =
            common::stream::push_job(redis, "ai.schedule.run", json!({ "run_id": run_id })).await
        {
            tracing::error!(run_id=%run_id, error=%error, "ai.schedule.run: push failed");
            if push_error.is_none() {
                push_error = Some(error);
            }
        }
    }
    if let Some(error) = push_error {
        return Err(error);
    }
    Ok(queued)
}

#[derive(sqlx::FromRow)]
struct RunRow {
    id: Uuid,
    schedule_id: Uuid,
    workspace_id: Uuid,
    prompt: String,
}

/// Execute one queued run; records success/failure and prunes old runs.
pub async fn run(pool: &PgPool, payload: Value) -> anyhow::Result<()> {
    let Some(run_id) = payload
        .get("run_id")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
    else {
        tracing::warn!(payload=%payload, "ai.schedule.run: missing run_id");
        return Ok(());
    };

    let claimed: Option<RunRow> = sqlx::query_as(
        "UPDATE ai_schedule_runs SET status = 'running', started_at = now() \
         WHERE id = $1 AND status = 'queued' \
         RETURNING id, schedule_id, workspace_id, prompt",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await?;
    let Some(run) = claimed else {
        tracing::warn!(run_id=%run_id, "ai.schedule.run: run already claimed or swept");
        return Ok(());
    };

    let (spec_value, created_by_id): (Option<Value>, Uuid) =
        sqlx::query_as("SELECT spec, created_by_id FROM ai_schedules WHERE id = $1")
            .bind(run.schedule_id)
            .fetch_one(pool)
            .await?;
    let allowed = match allowed_tools(spec_value) {
        Ok(allowed) => allowed,
        Err(error) => {
            tracing::error!(run_id=%run.id, error=%error, "ai.schedule.run: invalid schedule spec");
            finish_failed(pool, run.id, "invalid schedule spec").await?;
            prune_runs(pool, run.schedule_id).await?;
            return Ok(());
        }
    };

    let config = ai::resolve_llm_config(pool).await;
    if config.api_key.trim().is_empty() {
        finish_failed(pool, run.id, "AI is not configured for this instance").await?;
        prune_runs(pool, run.schedule_id).await?;
        return Ok(());
    }

    let trace = ai::agent::new_trace();
    let allowed_refs: Vec<&str> = allowed.iter().map(String::as_str).collect();
    let handle = ai::tools::read_tools(
        pool.clone(),
        run.workspace_id,
        created_by_id,
        trace.clone(),
        &allowed_refs,
    );
    let started = std::time::Instant::now();
    let result = tokio::time::timeout(
        ai::agent::AGENT_TIMEOUT,
        ai::agent::run_agent(
            &config.base_url,
            &config.api_key,
            &config.model,
            handle,
            None,
            &run.prompt,
        ),
    )
    .await;
    let result = match result {
        Ok(result) => result,
        Err(_) => {
            finish_failed(pool, run.id, "run exceeded the 180 second agent timeout").await?;
            prune_runs(pool, run.schedule_id).await?;
            return Ok(());
        }
    };

    match result {
        Ok(text) => {
            let calls = trace.lock().map(|c| c.clone()).unwrap_or_default();
            let tool_calls: Option<Value> = serde_json::to_value(&calls).ok();
            let updated = sqlx::query(
                "UPDATE ai_schedule_runs SET status = 'success', response = $2, response_html = $3, \
                 tool_calls = $4, finished_at = now() WHERE id = $1 AND status = 'running'",
            )
            .bind(run.id)
            .bind(&text)
            .bind(ai::response_html(&text))
            .bind(tool_calls)
            .execute(pool)
            .await?;
            if updated.rows_affected() == 0 {
                tracing::warn!(
                    run_id=%run.id,
                    "ai.schedule.run: run no longer running, success not recorded"
                );
            } else {
                insert_run_notification(pool, run.id).await?;
                tracing::info!(
                    run_id=%run.id,
                    schedule_id=%run.schedule_id,
                    workspace_id=%run.workspace_id,
                    duration_ms=%started.elapsed().as_millis(),
                    "ai.schedule.run finished"
                );
            }
        }
        Err(error) => {
            let message = match error {
                ai::LlmError::RateLimited => "rate limited by the model provider".to_string(),
                ai::LlmError::Upstream => "model provider request failed".to_string(),
            };
            finish_failed(pool, run.id, &message).await?;
        }
    }

    prune_runs(pool, run.schedule_id).await?;
    Ok(())
}

/// One in-app notification per scheduled run that reaches a terminal status.
/// Idempotent per run; skips manual runs and soft-deleted schedules.
pub async fn insert_run_notification(pool: &PgPool, run_id: Uuid) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO notifications (id, workspace_id, receiver_id, entity_name, entity_identifier, \
         title, sender, data, message_html, created_at, updated_at) \
         SELECT gen_random_uuid(), r.workspace_id, s.created_by_id, 'ai_schedule_run', r.id, \
                s.name, 'in_app:ai_schedule:run', \
                jsonb_build_object('ai_schedule', jsonb_build_object( \
                  'schedule_id', s.id, 'run_id', r.id, 'name', s.name, 'status', r.status, \
                  'finished_at', r.finished_at, 'error', r.error)), \
                '<p></p>', now(), now() \
         FROM ai_schedule_runs r JOIN ai_schedules s ON s.id = r.schedule_id \
         WHERE r.id = $1 AND r.trigger = 'scheduled' AND r.status IN ('success', 'failed') \
           AND s.deleted_at IS NULL \
           AND NOT EXISTS (SELECT 1 FROM notifications n WHERE n.entity_name = 'ai_schedule_run' \
                           AND n.entity_identifier = r.id)",
    )
    .bind(run_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Keep the newest 20 terminal runs per schedule; queued/running runs are
/// never pruned.
pub async fn prune_runs(pool: &PgPool, schedule_id: Uuid) -> anyhow::Result<()> {
    sqlx::query(
        "DELETE FROM ai_schedule_runs WHERE schedule_id = $1 \
         AND status NOT IN ('queued','running') \
         AND id NOT IN ( \
           SELECT id FROM ai_schedule_runs WHERE schedule_id = $1 \
           ORDER BY created_at DESC, id DESC LIMIT 20)",
    )
    .bind(schedule_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn finish_failed(pool: &PgPool, run_id: Uuid, message: &str) -> anyhow::Result<()> {
    let truncated: String = message.chars().take(500).collect();
    let updated = sqlx::query(
        "UPDATE ai_schedule_runs SET status = 'failed', error = $2, finished_at = now() \
         WHERE id = $1 AND status = 'running'",
    )
    .bind(run_id)
    .bind(truncated)
    .execute(pool)
    .await?;
    if updated.rows_affected() == 0 {
        tracing::warn!(
            run_id=%run_id,
            "ai.schedule.run: run no longer running, failure not recorded"
        );
    } else {
        insert_run_notification(pool, run_id).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::allowed_tools;
    use serde_json::json;

    #[test]
    fn allowed_tools_legacy_is_all_read_tools() {
        assert_eq!(
            allowed_tools(None).unwrap(),
            vec![
                "list_projects".to_string(),
                "count_work_items".to_string(),
                "search_work_items".to_string()
            ]
        );
    }

    #[test]
    fn allowed_tools_uses_validated_spec_subset() {
        let spec = json!({
            "version": 1,
            "description": "d",
            "how_to": ["step"],
            "tools": ["search_work_items", "list_projects"],
            "expected_output": "o"
        });
        assert_eq!(
            allowed_tools(Some(spec)).unwrap(),
            vec!["list_projects".to_string(), "search_work_items".to_string()]
        );
    }

    #[test]
    fn allowed_tools_rejects_invalid_spec() {
        let spec = json!({
            "version": 1,
            "description": "d",
            "how_to": [],
            "tools": [],
            "expected_output": "o"
        });
        assert!(allowed_tools(Some(spec)).is_err());
    }
}
