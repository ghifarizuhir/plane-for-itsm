//! AI schedule jobs: claim due schedules (`tick`) and run the agent (`run`).

use ai::schedule::ScheduleProposal;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
struct DueSchedule {
    id: Uuid,
    frequency: String,
    time_of_day: String,
    day_of_week: Option<i16>,
    day_of_month: Option<i16>,
    timezone: String,
}

impl DueSchedule {
    fn proposal(&self, prompt: &str) -> Result<ScheduleProposal, String> {
        ScheduleProposal::new(
            "scheduled",
            prompt,
            &self.frequency,
            Some(&self.time_of_day),
            self.day_of_week,
            self.day_of_month,
            Some(&self.timezone),
        )
    }
}

/// Mark runs stuck in queued/running for over 15 minutes as failed.
async fn sweep_stuck_runs(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE ai_schedule_runs SET status = 'failed', \
         error = 'run did not finish within 15 minutes', finished_at = now() \
         WHERE status IN ('queued','running') AND created_at < now() - interval '15 minutes'",
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
    let due: Vec<DueSchedule> = sqlx::query_as(
        "SELECT id, frequency, time_of_day, day_of_week, day_of_month, timezone \
         FROM ai_schedules \
         WHERE enabled = true AND deleted_at IS NULL AND next_run_at <= now() \
         ORDER BY next_run_at LIMIT 50 FOR UPDATE SKIP LOCKED",
    )
    .fetch_all(&mut *tx)
    .await?;

    let mut queued = Vec::new();
    for schedule in &due {
        let prompt: String = sqlx::query_scalar("SELECT prompt FROM ai_schedules WHERE id = $1")
            .bind(schedule.id)
            .fetch_one(&mut *tx)
            .await?;
        let Ok(proposal) = schedule.proposal(&prompt) else {
            // DB constraints make this unreachable; stay defensive and move the
            // schedule forward so a bad row can never wedge the tick loop.
            tracing::warn!(schedule_id=%schedule.id, "ai.schedule.tick: invalid preset, skipping");
            sqlx::query(
                "UPDATE ai_schedules SET next_run_at = now() + interval '1 hour', updated_at = now() WHERE id = $1",
            )
            .bind(schedule.id)
            .execute(&mut *tx)
            .await?;
            continue;
        };
        let next = proposal.next_occurrence(Utc::now());
        sqlx::query("UPDATE ai_schedules SET next_run_at = $2, updated_at = now() WHERE id = $1")
            .bind(schedule.id)
            .bind(next)
            .execute(&mut *tx)
            .await?;
        let run_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO ai_schedule_runs (id, schedule_id, workspace_id, status, trigger, prompt, created_at) \
             SELECT $1, s.id, s.workspace_id, 'queued', 'scheduled', s.prompt, now() \
             FROM ai_schedules s WHERE s.id = $2",
        )
        .bind(run_id)
        .bind(schedule.id)
        .execute(&mut *tx)
        .await?;
        queued.push(run_id);
    }
    tx.commit().await?;

    for run_id in &queued {
        common::stream::push_job(redis, "ai.schedule.run", json!({ "run_id": run_id })).await?;
    }
    Ok(queued)
}

/// Execute a queued run (implemented in Task 8).
pub async fn run(pool: &PgPool, payload: Value) -> anyhow::Result<()> {
    let _ = (pool, payload);
    Ok(())
}
