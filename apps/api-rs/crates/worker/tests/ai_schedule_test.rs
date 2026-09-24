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

#[tokio::test]
async fn prune_keeps_newest_twenty_terminal_runs() {
    let pool = pool().await;
    let slug = format!("aisp-{}", Uuid::new_v4().simple());
    let workspace_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let schedule_id = Uuid::new_v4();
    insert_user(&pool, user_id, &slug).await;
    sqlx::query(
        "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, background_color) \
         VALUES ($1, 'AI Prune', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
    )
    .bind(workspace_id).bind(&slug).bind(user_id).execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO ai_schedules (id, workspace_id, created_by_id, name, prompt, frequency, time_of_day, \
         timezone, enabled, next_run_at, proposal_key, created_at, updated_at) \
         VALUES ($1, $2, $3, 'Daily', 'Summarize', 'daily', '09:00', 'UTC', true, now(), $4, now(), now())",
    )
    .bind(schedule_id).bind(workspace_id).bind(user_id).bind(Uuid::new_v4()).execute(&pool).await.unwrap();

    // 22 terminal runs, oldest first, plus one queued run that must survive.
    for index in 0..22 {
        sqlx::query(
            "INSERT INTO ai_schedule_runs (id, schedule_id, workspace_id, status, trigger, prompt, created_at, finished_at) \
             VALUES ($1, $2, $3, 'success', 'scheduled', 'Summarize', now() - make_interval(secs => $4), now())",
        )
        .bind(Uuid::new_v4()).bind(schedule_id).bind(workspace_id).bind(1000 - index)
        .execute(&pool).await.unwrap();
    }
    let queued_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO ai_schedule_runs (id, schedule_id, workspace_id, status, trigger, prompt, created_at) \
         VALUES ($1, $2, $3, 'queued', 'scheduled', 'Summarize', now() - interval '1 day')",
    )
    .bind(queued_id).bind(schedule_id).bind(workspace_id).execute(&pool).await.unwrap();

    ai_schedule::prune_runs(&pool, schedule_id)
        .await
        .expect("prune");

    let terminal: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ai_schedule_runs WHERE schedule_id = $1 AND status = 'success'",
    )
    .bind(schedule_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(terminal, 20, "newest 20 terminal runs retained");
    let queued_survived: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM ai_schedule_runs WHERE id = $1)")
            .bind(queued_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(queued_survived, "queued runs are never pruned");

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
async fn job_by_id_rejects_partial_ids_and_malformed_entries() {
    let mut redis = common::redis::create_redis(
        &std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into()),
    )
    .await;
    // Add a well-formed and a malformed entry with explicit ids.
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let good_id = format!("{ms}-1");
    let bad_id = format!("{ms}-2");
    let _: String = redis::cmd("XADD")
        .arg(common::stream::STREAM)
        .arg(&good_id)
        .arg("job")
        .arg("ai.schedule.tick")
        .arg("payload")
        .arg("{}")
        .query_async(&mut redis)
        .await
        .unwrap();
    let _: String = redis::cmd("XADD")
        .arg(common::stream::STREAM)
        .arg(&bad_id)
        .arg("job")
        .arg("ai.schedule.tick")
        .query_async(&mut redis)
        .await
        .unwrap();

    // Partial millisecond id must not resolve to the first entry of that ms.
    let partial = common::stream::job_by_id(&mut redis, &ms.to_string())
        .await
        .unwrap();
    assert!(partial.is_none(), "partial ids must not resolve");

    // Exact id resolves.
    let exact = common::stream::job_by_id(&mut redis, &good_id)
        .await
        .unwrap();
    assert!(exact.is_some());

    // Found-but-malformed is an error, not a silent skip.
    let malformed = common::stream::job_by_id(&mut redis, &bad_id).await;
    assert!(malformed.is_err(), "malformed entries must error");
}

#[tokio::test]
async fn handle_by_id_skips_disabled_jobs() {
    let pool = pool().await;
    let mut redis = common::redis::create_redis(
        &std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into()),
    )
    .await;
    let id: String = redis::cmd("XADD")
        .arg(common::stream::STREAM)
        .arg("*")
        .arg("job")
        .arg("email.notification")
        .arg("payload")
        .arg("{}")
        .query_async(&mut redis)
        .await
        .unwrap();

    worker::handlers::handle_by_id(&pool, &mut redis, &id)
        .await
        .expect("disabled jobs are skipped without error");
}

async fn spawn_fake_upstream() -> String {
    use axum::{routing::post, Json, Router};
    async fn handler(
        Json(_body): Json<serde_json::Value>,
    ) -> (axum::http::StatusCode, Json<serde_json::Value>) {
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({
                "id": "chatcmpl-test",
                "object": "chat.completion",
                "created": 0,
                "model": "test",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "scheduled answer"},
                    "finish_reason": "stop"
                }]
            })),
        )
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/v1/chat/completions", post(handler)),
        )
        .await
        .unwrap();
    });
    format!("http://{addr}/v1")
}

#[tokio::test]
async fn run_success_records_response_and_prunes() {
    let pool = pool().await;
    let slug = format!("aisok-{}", Uuid::new_v4().simple());
    let workspace_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let schedule_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    insert_user(&pool, user_id, &slug).await;
    sqlx::query(
        "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, background_color) \
         VALUES ($1, 'AI Success', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
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
         VALUES ($1, $2, $3, 'Daily', 'Summarize overdue', 'daily', '09:00', 'UTC', true, now() + interval '1 day', $4, now(), now())",
    )
    .bind(schedule_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap();

    // 21 older terminal runs so the prune has work to do
    for index in 0..21 {
        sqlx::query(
            "INSERT INTO ai_schedule_runs (id, schedule_id, workspace_id, status, trigger, prompt, created_at, finished_at) \
             VALUES ($1, $2, $3, 'success', 'scheduled', 'old', now() - make_interval(secs => $4), now())",
        )
        .bind(Uuid::new_v4())
        .bind(schedule_id)
        .bind(workspace_id)
        .bind(2000 - index)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO ai_schedule_runs (id, schedule_id, workspace_id, status, trigger, prompt, created_at) \
         VALUES ($1, $2, $3, 'queued', 'scheduled', 'Summarize overdue', now())",
    )
    .bind(run_id)
    .bind(schedule_id)
    .bind(workspace_id)
    .execute(&pool)
    .await
    .unwrap();

    let base_url = spawn_fake_upstream().await;
    std::env::set_var("SKIP_ENV_VAR", "0");
    std::env::set_var("LLM_API_KEY", "test-key");
    std::env::set_var("LLM_BASE_URL", &base_url);
    std::env::set_var("LLM_MODEL", "test-model");
    ai_schedule::run(&pool, serde_json::json!({ "run_id": run_id }))
        .await
        .expect("run");

    let (status, response, response_html, finished): (
        String,
        Option<String>,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT status, response, response_html, finished_at FROM ai_schedule_runs WHERE id = $1",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "success");
    assert_eq!(response.as_deref(), Some("scheduled answer"));
    assert_eq!(response_html.as_deref(), Some("scheduled answer"));
    assert!(finished.is_some());

    let remaining: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ai_schedule_runs WHERE schedule_id = $1 AND status = 'success'",
    )
    .bind(schedule_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(remaining, 20, "retention keeps the newest 20 terminal runs");

    std::env::remove_var("SKIP_ENV_VAR");
    std::env::remove_var("LLM_API_KEY");
    std::env::remove_var("LLM_BASE_URL");
    std::env::remove_var("LLM_MODEL");
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
async fn tick_sweeps_stuck_runs() {
    let pool = pool().await;
    let slug = format!("aisw-{}", Uuid::new_v4().simple());
    let workspace_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let schedule_id = Uuid::new_v4();
    insert_user(&pool, user_id, &slug).await;
    sqlx::query(
        "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, background_color) \
         VALUES ($1, 'AI Sweep', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
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
         VALUES ($1, $2, $3, 'Daily', 'Summarize', 'daily', '09:00', 'UTC', true, now() + interval '1 day', $4, now(), now())",
    )
    .bind(schedule_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap();

    let stuck_running = Uuid::new_v4();
    let stuck_queued = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO ai_schedule_runs (id, schedule_id, workspace_id, status, trigger, prompt, created_at, started_at) \
         VALUES ($1, $2, $3, 'running', 'scheduled', 'x', now() - interval '1 hour', now() - interval '20 minutes')",
    )
    .bind(stuck_running)
    .bind(schedule_id)
    .bind(workspace_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_schedule_runs (id, schedule_id, workspace_id, status, trigger, prompt, created_at) \
         VALUES ($1, $2, $3, 'queued', 'scheduled', 'x', now() - interval '7 hours')",
    )
    .bind(stuck_queued)
    .bind(schedule_id)
    .bind(workspace_id)
    .execute(&pool)
    .await
    .unwrap();

    let mut redis = common::redis::create_redis(
        &std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into()),
    )
    .await;
    ai_schedule::tick(&pool, &mut redis).await.expect("tick");

    let (running_status, running_error): (String, Option<String>) =
        sqlx::query_as("SELECT status, error FROM ai_schedule_runs WHERE id = $1")
            .bind(stuck_running)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(running_status, "failed");
    assert!(running_error.unwrap().contains("did not finish"));
    let (queued_status, queued_error): (String, Option<String>) =
        sqlx::query_as("SELECT status, error FROM ai_schedule_runs WHERE id = $1")
            .bind(stuck_queued)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(queued_status, "failed");
    assert!(queued_error.unwrap().contains("never started"));

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
