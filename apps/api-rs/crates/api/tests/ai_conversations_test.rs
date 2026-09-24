//! DB-backed tests for the Galileo conversation endpoints.

use api::middleware::auth::AuthUser;
use api::routes::ai_conversations;
use api::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use common::config::AppConfig;
use serde_json::json;
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
        let slug = format!("aich-{}", Uuid::new_v4().simple());
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        insert_user(pool, user_id, &slug).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'AI Chat', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
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
        sqlx::query("DELETE FROM ai_conversations WHERE workspace_id = $1")
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
async fn create_and_list_are_owner_scoped() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let other = scratch.add_actor(&pool, 20).await;

    let (status, Json(created)) = ai_conversations::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"mode": "agent"})),
    )
    .await
    .expect("create");
    assert_eq!(status, StatusCode::CREATED);
    assert!(Uuid::parse_str(created["id"].as_str().unwrap()).is_ok());
    assert_eq!(created["mode"], json!("agent"));
    assert_eq!(created["title"], json!(""));

    let (status, Json(list)) = ai_conversations::list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
    )
    .await
    .expect("list");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["conversations"].as_array().unwrap().len(), 1);

    // Another workspace member cannot see it.
    let (_, Json(other_list)) = ai_conversations::list(
        State(st.clone()),
        AuthUser(other),
        Path(scratch.slug.clone()),
    )
    .await
    .expect("other list");
    assert_eq!(other_list["conversations"].as_array().unwrap().len(), 0);

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn creating_more_than_fifty_conversations_prunes_the_oldest() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    for index in 0..51 {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO ai_conversations (id, workspace_id, created_by_id, mode, title, created_at, updated_at) \
             VALUES ($1, $2, $3, 'classic', $4, now() - make_interval(secs => $5), now() - make_interval(secs => $5))",
        )
        .bind(id)
        .bind(scratch.workspace_id)
        .bind(scratch.user_id)
        .bind(format!("old-{index}"))
        .bind(5000 - index)
        .execute(&pool)
        .await
        .unwrap();
    }

    let (status, _) = ai_conversations::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"mode": "classic"})),
    )
    .await
    .expect("create");
    assert_eq!(status, StatusCode::CREATED);

    let (_, Json(list)) = ai_conversations::list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
    )
    .await
    .expect("list");
    let conversations = list["conversations"].as_array().unwrap();
    assert_eq!(conversations.len(), 50);
    // Newest (empty title) stays.
    assert_eq!(conversations[0]["title"], json!(""));

    // Assert the DB itself, not just the LIMIT-50 list: the prune must have
    // deleted the two oldest rows.
    let stored: i64 = sqlx::query_scalar(
        "SELECT count(*)::int8 FROM ai_conversations WHERE workspace_id = $1 AND created_by_id = $2",
    )
    .bind(scratch.workspace_id)
    .bind(scratch.user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, 50);
    let oldest: i64 = sqlx::query_scalar(
        "SELECT count(*)::int8 FROM ai_conversations WHERE workspace_id = $1 AND title = 'old-0'",
    )
    .bind(scratch.workspace_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(oldest, 0);

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn pruning_is_scoped_to_the_owner() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let other = scratch.add_actor(&pool, 20).await;

    // 51 conversations for the owner + 1 for the other member.
    for index in 0..51 {
        sqlx::query(
            "INSERT INTO ai_conversations (id, workspace_id, created_by_id, mode, title, created_at, updated_at)              VALUES ($1, $2, $3, 'classic', $4, now() - make_interval(secs => $5), now() - make_interval(secs => $5))",
        )
        .bind(Uuid::new_v4())
        .bind(scratch.workspace_id)
        .bind(scratch.user_id)
        .bind(format!("mine-{index}"))
        .bind(5000 - index)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO ai_conversations (id, workspace_id, created_by_id, mode, title, created_at, updated_at)          VALUES ($1, $2, $3, 'classic', 'theirs', now() - interval '10 seconds', now() - interval '10 seconds')",
    )
    .bind(Uuid::new_v4())
    .bind(scratch.workspace_id)
    .bind(other)
    .execute(&pool)
    .await
    .unwrap();

    let (status, _) = ai_conversations::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"mode": "classic"})),
    )
    .await
    .expect("create");
    assert_eq!(status, StatusCode::CREATED);

    let mine: i64 =
        sqlx::query_scalar("SELECT count(*)::int8 FROM ai_conversations WHERE created_by_id = $1")
            .bind(scratch.user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let theirs: i64 =
        sqlx::query_scalar("SELECT count(*)::int8 FROM ai_conversations WHERE created_by_id = $1")
            .bind(other)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(mine, 50);
    assert_eq!(theirs, 1, "another member's conversations are untouched");

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn create_trims_and_truncates_the_optional_title() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let long_title = format!("  {}  ", "x".repeat(150));
    let (status, Json(created)) = ai_conversations::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"mode": "classic", "title": long_title})),
    )
    .await
    .expect("create");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["title"].as_str().unwrap().chars().count(), 120);
    assert!(!created["title"].as_str().unwrap().starts_with(' '));

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn invalid_mode_is_rejected() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, _) = ai_conversations::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"mode": "turbo"})),
    )
    .await
    .expect("create");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    scratch.purge(&pool).await;
}
