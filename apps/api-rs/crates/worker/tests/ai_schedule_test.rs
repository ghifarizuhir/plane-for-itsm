//! DB-backed tests for the schedule tick handler. Gated on DATABASE_URL and
//! REDIS_URL (defaults target the local compose stack).

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
    let pushed = ai_schedule::tick(&pool, &mut redis).await.expect("tick");

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
    assert_eq!(pushed.len(), 1, "one run queued");
    let (status, trigger): (String, String) =
        sqlx::query_as("SELECT status, trigger FROM ai_schedule_runs WHERE id = $1")
            .bind(pushed[0])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "queued");
    assert_eq!(trigger, "scheduled");

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
    sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1")
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
