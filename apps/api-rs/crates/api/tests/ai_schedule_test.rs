//! DB-backed tests for the AI scheduler endpoints.

use api::middleware::auth::AuthUser;
use api::routes::ai_schedule;
use api::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use common::config::AppConfig;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

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

async fn state(pool: &PgPool) -> AppState {
    AppState {
        pool: pool.clone(),
        redis: redis::Client::open("redis://127.0.0.1:6379").expect("redis client"),
        config: AppConfig::from_env(),
    }
}

fn create_body(proposal_key: Uuid) -> Value {
    json!({
        "name": "Daily report",
        "prompt": "Summarize overdue work items",
        "frequency": "daily",
        "time": "09:00",
        "timezone": "UTC",
        "proposal_key": proposal_key,
    })
}

struct Scratch {
    slug: String,
    workspace_id: Uuid,
    user_id: Uuid,
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

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let slug = format!("aisc-{}", Uuid::new_v4().simple());
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        insert_user(pool, user_id, &slug).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'AI Sched', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
        )
        .bind(workspace_id)
        .bind(&slug)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("scratch workspace");
        sqlx::query(
            "INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, \
             workspace_id, view_props, default_props, issue_props, explored_features, \
             getting_started_checklist, tips, is_active) \
             VALUES (gen_random_uuid(), now(), now(), 20, $1, $2, '{}', '{}', '{}', '{}', '{}', \
             '{}', true)",
        )
        .bind(user_id)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch workspace member");
        Self {
            slug,
            workspace_id,
            user_id,
        }
    }

    async fn add_actor(&self, pool: &PgPool, workspace_role: i16) -> Uuid {
        let user_id = Uuid::new_v4();
        let username = format!("{}-{}", self.slug, &user_id.simple().to_string()[..8]);
        insert_user(pool, user_id, &username).await;
        sqlx::query(
            "INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, \
             workspace_id, view_props, default_props, issue_props, explored_features, \
             getting_started_checklist, tips, is_active) \
             VALUES (gen_random_uuid(), now(), now(), $1, $2, $3, '{}', '{}', '{}', '{}', '{}', \
             '{}', true)",
        )
        .bind(workspace_role)
        .bind(user_id)
        .bind(self.workspace_id)
        .execute(pool)
        .await
        .expect("scratch actor");
        user_id
    }

    async fn purge(&self, pool: &PgPool) {
        sqlx::query("DELETE FROM ai_schedule_runs WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM ai_schedules WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM users WHERE username LIKE $1")
            .bind(format!("{}%", self.slug))
            .execute(pool)
            .await
            .ok();
    }
}

#[tokio::test]
async fn create_is_idempotent_and_list_returns_rows() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let state = state(&pool).await;

    let body = json!({
        "name": "Weekly backlog",
        "prompt": "Summarize backlog",
        "frequency": "weekly",
        "time": "09:00",
        "day_of_week": 1,
        "timezone": "Asia/Jakarta",
        "proposal_key": Uuid::new_v4(),
    });
    let (status, Json(created)) = ai_schedule::create(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(body.clone()),
    )
    .await
    .expect("create ok");
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap().to_string();

    // same proposal_key returns the same row (idempotent)
    let (status, Json(again)) = ai_schedule::create(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(body),
    )
    .await
    .expect("replay ok");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again["id"], created["id"]);

    let (status, Json(list)) = ai_schedule::list(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
    )
    .await
    .expect("list ok");
    assert_eq!(status, StatusCode::OK);
    let rows = list.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], json!(id));
    assert_eq!(rows[0]["frequency"], json!("weekly"));
    assert_eq!(rows[0]["day_of_week"], json!(1));

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn create_rejects_guests_and_bad_payloads() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let state = state(&pool).await;
    let guest = scratch.add_actor(&pool, 5).await;

    let bad = json!({
        "name": "x",
        "prompt": "y",
        "frequency": "sometimes",
        "proposal_key": Uuid::new_v4(),
    });
    let (status, _) = ai_schedule::create(
        State(state.clone()),
        AuthUser(guest),
        Path(scratch.slug.clone()),
        Json(bad.clone()),
    )
    .await
    .expect("guest handled");
    assert_eq!(status, StatusCode::FORBIDDEN);

    let member = scratch.add_actor(&pool, 15).await;
    let (status, Json(err)) = ai_schedule::create(
        State(state.clone()),
        AuthUser(member),
        Path(scratch.slug.clone()),
        Json(bad),
    )
    .await
    .expect("bad payload handled");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(err["error"].as_str().unwrap().contains("frequency"));

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn detail_returns_schedule_with_runs() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let state = state(&pool).await;
    let member = scratch.add_actor(&pool, 15).await;

    let (_, Json(created)) = ai_schedule::create(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body(Uuid::new_v4())),
    )
    .await
    .unwrap();
    let schedule_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    sqlx::query(
        "INSERT INTO ai_schedule_runs (id, schedule_id, workspace_id, status, trigger, prompt, response, response_html, created_at, finished_at) \
         VALUES ($1, $2, $3, 'success', 'scheduled', 'Summarize overdue work items', 'done', 'done', now(), now())",
    )
    .bind(Uuid::new_v4())
    .bind(schedule_id)
    .bind(scratch.workspace_id)
    .execute(&pool)
    .await
    .unwrap();

    // members can read detail
    let (status, Json(detail)) = ai_schedule::detail(
        State(state.clone()),
        AuthUser(member),
        Path((scratch.slug.clone(), schedule_id)),
    )
    .await
    .expect("detail ok");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["id"], json!(schedule_id));
    assert_eq!(detail["frequency"], json!("daily"));
    let runs = detail["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["status"], json!("success"));
    assert_eq!(runs[0]["trigger"], json!("scheduled"));
    assert_eq!(runs[0]["response_html"], json!("done"));

    // unknown id -> 404
    let (status, _) = ai_schedule::detail(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), Uuid::new_v4())),
    )
    .await
    .expect("missing handled");
    assert_eq!(status, StatusCode::NOT_FOUND);

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn patch_delete_and_run_now_follow_creator_or_admin() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let state = state(&pool).await;
    let member = scratch.add_actor(&pool, 15).await;
    let admin = scratch.add_actor(&pool, 20).await;

    let (_, Json(created)) = ai_schedule::create(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body(Uuid::new_v4())),
    )
    .await
    .unwrap();
    let schedule_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    // non-creator member cannot pause
    let (status, _) = ai_schedule::patch(
        State(state.clone()),
        AuthUser(member),
        Path((scratch.slug.clone(), schedule_id)),
        Json(json!({"enabled": false})),
    )
    .await
    .expect("patch handled");
    assert_eq!(status, StatusCode::FORBIDDEN);

    // creator can pause, then resume recomputes next_run_at
    let (status, Json(patched)) = ai_schedule::patch(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
        Json(json!({"enabled": false})),
    )
    .await
    .expect("pause ok");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched["enabled"], json!(false));

    let (status, Json(resumed)) = ai_schedule::patch(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
        Json(json!({"enabled": true})),
    )
    .await
    .expect("resume ok");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resumed["enabled"], json!(true));
    let next: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT next_run_at FROM ai_schedules WHERE id = $1")
            .bind(schedule_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        next > chrono::Utc::now(),
        "resume must recompute a future next_run_at"
    );

    // workspace admin (not creator) can also manage
    let (status, _) = ai_schedule::patch(
        State(state.clone()),
        AuthUser(admin),
        Path((scratch.slug.clone(), schedule_id)),
        Json(json!({"enabled": false})),
    )
    .await
    .expect("admin patch ok");
    assert_eq!(status, StatusCode::OK);

    // non-creator member cannot delete
    let (status, _) = ai_schedule::destroy(
        State(state.clone()),
        AuthUser(member),
        Path((scratch.slug.clone(), schedule_id)),
    )
    .await
    .expect("delete handled");
    assert_eq!(status, StatusCode::FORBIDDEN);

    // run now as creator queues a manual run
    let (status, Json(run)) = ai_schedule::run_now(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
    )
    .await
    .expect("run now ok");
    assert_eq!(status, StatusCode::CREATED);
    let run_id = Uuid::parse_str(run["run_id"].as_str().unwrap()).unwrap();
    let (status_db, trigger): (String, String) =
        sqlx::query_as("SELECT status, trigger FROM ai_schedule_runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status_db, "queued");
    assert_eq!(trigger, "manual");

    // creator deletes -> soft delete; detail and list no longer show it
    let (status, _) = ai_schedule::destroy(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
    )
    .await
    .expect("delete ok");
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = ai_schedule::detail(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), schedule_id)),
    )
    .await
    .expect("detail after delete");
    assert_eq!(status, StatusCode::NOT_FOUND);

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn soft_delete_frees_proposal_key_and_list_isolates_workspaces() {
    let pool = pool().await;
    let first = Scratch::new(&pool).await;
    let second = Scratch::new(&pool).await;
    let state = state(&pool).await;
    let key = Uuid::new_v4();

    let (_, Json(created)) = ai_schedule::create(
        State(state.clone()),
        AuthUser(first.user_id),
        Path(first.slug.clone()),
        Json(create_body(key)),
    )
    .await
    .unwrap();
    let schedule_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    // same key in another workspace is a distinct schedule
    let (status, Json(other)) = ai_schedule::create(
        State(state.clone()),
        AuthUser(second.user_id),
        Path(second.slug.clone()),
        Json(create_body(key)),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::CREATED);
    assert_ne!(other["id"], created["id"]);

    // soft delete the first, then reuse its key in the same workspace
    let (status, _) = ai_schedule::destroy(
        State(state.clone()),
        AuthUser(first.user_id),
        Path((first.slug.clone(), schedule_id)),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, Json(recreated)) = ai_schedule::create(
        State(state.clone()),
        AuthUser(first.user_id),
        Path(first.slug.clone()),
        Json(create_body(key)),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::CREATED);
    assert_ne!(recreated["id"], created["id"]);

    // list isolation
    let (_, Json(list)) = ai_schedule::list(
        State(state.clone()),
        AuthUser(first.user_id),
        Path(first.slug.clone()),
    )
    .await
    .unwrap();
    let rows = list.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], recreated["id"]);

    first.purge(&pool).await;
    second.purge(&pool).await;
}

#[tokio::test]
async fn list_rejects_guests() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let state = state(&pool).await;
    let guest = scratch.add_actor(&pool, 5).await;

    let (status, _) = ai_schedule::list(
        State(state.clone()),
        AuthUser(guest),
        Path(scratch.slug.clone()),
    )
    .await
    .expect("guest handled");
    assert_eq!(status, StatusCode::FORBIDDEN);

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn create_enforces_twenty_schedule_limit() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let state = state(&pool).await;
    for _ in 0..20 {
        let body = json!({
            "name": "Daily",
            "prompt": "Report",
            "frequency": "daily",
            "time": "09:00",
            "timezone": "UTC",
            "proposal_key": Uuid::new_v4(),
        });
        let (status, _) = ai_schedule::create(
            State(state.clone()),
            AuthUser(scratch.user_id),
            Path(scratch.slug.clone()),
            Json(body),
        )
        .await
        .expect("create ok");
        assert_eq!(status, StatusCode::CREATED);
    }
    let (status, Json(err)) = ai_schedule::create(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "name": "One too many",
            "prompt": "Report",
            "frequency": "daily",
            "proposal_key": Uuid::new_v4(),
        })),
    )
    .await
    .expect("limit handled");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(err["error"].as_str().unwrap().contains("20"));

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn replay_at_capacity_returns_existing_schedule() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let state = state(&pool).await;

    let mut first_key = None;
    for index in 0..20 {
        let key = Uuid::new_v4();
        if index == 0 {
            first_key = Some(key);
        }
        let (status, _) = ai_schedule::create(
            State(state.clone()),
            AuthUser(scratch.user_id),
            Path(scratch.slug.clone()),
            Json(json!({
                "name": format!("Daily {index}"),
                "prompt": "Report",
                "frequency": "daily",
                "proposal_key": key,
            })),
        )
        .await
        .expect("create ok");
        assert_eq!(status, StatusCode::CREATED);
    }

    // Replaying an existing key at 20/20 must return the original schedule.
    let (status, Json(replay)) = ai_schedule::create(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "name": "replayed",
            "prompt": "Report",
            "frequency": "daily",
            "proposal_key": first_key.unwrap(),
        })),
    )
    .await
    .expect("replay handled");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["already_exists"], json!(true));

    // A brand-new key at 20/20 still hits the cap.
    let (status, Json(err)) = ai_schedule::create(
        State(state.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "name": "One too many",
            "prompt": "Report",
            "frequency": "daily",
            "proposal_key": Uuid::new_v4(),
        })),
    )
    .await
    .expect("cap handled");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(err["error"].as_str().unwrap().contains("20"));

    scratch.purge(&pool).await;
}
