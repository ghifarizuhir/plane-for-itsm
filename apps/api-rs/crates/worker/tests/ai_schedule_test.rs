//! DB-backed tests for the schedule tick handler.
//!
//! Requires Postgres (DATABASE_URL, default `postgres://plane:plane@localhost:5432/plane`)
//! and Redis (REDIS_URL, default `redis://127.0.0.1:6379`) — the local compose
//! stack. A live worker may race the pushed job once deployed, and the shared
//! database is mutated by other tests, so run with `--test-threads=1`.

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;
use worker::handlers::ai_schedule;

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://plane:plane@localhost:5432/plane".into())
}

async fn pool() -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url())
        .await
        .expect("test database must be reachable (set DATABASE_URL)")
}

async fn insert_user(pool: &PgPool, user_id: Uuid, username: &str) {
    sqlx::query(
        "INSERT INTO users (id, password, username, email, first_name, last_name, avatar, \
         date_joined, created_at, updated_at, last_location, created_location, is_superuser, \
         is_managed, is_password_expired, is_active, is_staff, is_email_verified, \
         is_password_autoset, token, user_timezone, last_login_ip, last_logout_ip, \
         last_login_medium, last_login_uagent, is_bot, display_name, is_email_valid, \
         is_password_reset_required) \
         VALUES ($1, '', $2, $3, '', '', '', now(), now(), now(), '', '', false, false, \
         false, true, false, false, true, $4, 'UTC', '', '', '', '', false, $2, true, false)",
    )
    .bind(user_id)
    .bind(username)
    .bind(format!("{username}@example.invalid"))
    .bind(Uuid::new_v4().simple().to_string())
    .execute(pool)
    .await
    .expect("scratch user");
}

#[tokio::test]
async fn tick_claims_due_schedules_and_queues_runs() {
    let pool = pool().await;
    let slug = format!("aisc-{}", Uuid::new_v4().simple());
    let workspace_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let schedule_id = Uuid::new_v4();
    let proposal_key = Uuid::new_v4();
    insert_user(&pool, user_id, &slug).await;
    sqlx::query(
        "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
         background_color) VALUES ($1, 'AI Sched', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
    )
    .bind(workspace_id)
    .bind(&slug)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    // schedule due one minute in the past
    sqlx::query(
        "INSERT INTO ai_schedules (id, workspace_id, created_by_id, name, prompt, frequency, \
         time_of_day, timezone, enabled, next_run_at, proposal_key, created_at, updated_at) \
         VALUES ($1, $2, $3, 'Daily', 'Summarize overdue', 'daily', '09:00', 'UTC', true, \
         now() - interval '1 minute', $4, now(), now())",
    )
    .bind(schedule_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(proposal_key)
    .execute(&pool)
    .await
    .unwrap();

    let mut redis = common::redis::create_redis(
        &std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into()),
    )
    .await;
    let _first = ai_schedule::tick(&pool, &mut redis)
        .await
        .expect("first tick");
    let second = ai_schedule::tick(&pool, &mut redis)
        .await
        .expect("second tick");

    let next: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT next_run_at FROM ai_schedules WHERE id = $1")
            .bind(schedule_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        next > Utc::now(),
        "next_run_at must advance into the future"
    );

    let runs: Vec<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, status, trigger FROM ai_schedule_runs WHERE schedule_id = $1")
            .bind(schedule_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        runs.len(),
        1,
        "exactly one run for this schedule across two ticks"
    );
    assert_eq!(runs[0].2, "scheduled");
    assert!(
        !second.contains(&runs[0].0),
        "second tick must not re-claim"
    );
    assert!(
        ["queued", "running", "success", "failed"].contains(&runs[0].1.as_str()),
        "unexpected run status {}",
        runs[0].1
    );

    // cleanup
    sqlx::query("DELETE FROM ai_schedule_runs WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM ai_schedules WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn run_marks_failed_when_llm_is_not_configured() {
    let pool = pool().await;
    // Reuse seeding from tick test: create workspace + schedule + queued run.
    let slug = format!("aisr-{}", Uuid::new_v4().simple());
    let workspace_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let schedule_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    insert_user(&pool, user_id, &slug).await;
    sqlx::query(
        "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, background_color) \
         VALUES ($1, 'AI Run', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
    )
    .bind(workspace_id)
    .bind(&slug)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_schedules (id, workspace_id, created_by_id, name, prompt, frequency, time_of_day, \
         timezone, enabled, next_run_at, proposal_key, created_at, updated_at) \
         VALUES ($1, $2, $3, 'Daily', 'Summarize', 'daily', '09:00', 'UTC', true, now(), $4, now(), now())",
    )
    .bind(schedule_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_schedule_runs (id, schedule_id, workspace_id, status, trigger, prompt, created_at) \
         VALUES ($1, $2, $3, 'queued', 'scheduled', 'Summarize', now())",
    )
    .bind(run_id)
    .bind(schedule_id)
    .bind(workspace_id)
    .execute(&pool)
    .await
    .unwrap();

    // With LLM_API_KEY unset and SKIP_ENV_VAR=0, config resolves empty.
    std::env::set_var("SKIP_ENV_VAR", "0");
    std::env::remove_var("LLM_API_KEY");
    ai_schedule::run(&pool, serde_json::json!({ "run_id": run_id }))
        .await
        .expect("run handled");

    let (status, error): (String, Option<String>) =
        sqlx::query_as("SELECT status, error FROM ai_schedule_runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "failed");
    assert!(error.unwrap().contains("not configured"));

    // cleanup (mirror the tick test's cleanup for this workspace)
    std::env::remove_var("SKIP_ENV_VAR");
    sqlx::query("DELETE FROM ai_schedule_runs WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM ai_schedules WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
}
