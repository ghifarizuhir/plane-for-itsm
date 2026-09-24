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

/// Mark runs stuck in queued/running as failed. `running` uses `started_at`
/// (agent timeout is 180 s, so 15 minutes means genuinely stuck); `queued`
/// uses a horizon larger than the worst-case serial drain of one tick batch.
async fn sweep_stuck_runs(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE ai_schedule_runs SET status = 'failed', \
         error = 'run did not finish within 15 minutes', finished_at = now() \
         WHERE status = 'running' AND started_at < now() - interval '15 minutes'",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE ai_schedule_runs SET status = 'failed', \
         error = 'run was never started (worker backlog or lost job)', finished_at = now() \
         WHERE status = 'queued' AND created_at < now() - interval '6 hours'",
    )
    .execute(pool)
    .await?;
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

/// Execute a queued run (implemented in Task 8).
pub async fn run(pool: &PgPool, payload: Value) -> anyhow::Result<()> {
    let _ = (pool, payload);
    Ok(())
}
