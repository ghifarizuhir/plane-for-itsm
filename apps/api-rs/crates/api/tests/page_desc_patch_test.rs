//! Regression tests for the page description PATCH handler
//! (`page::desc_patch`): Django's `Page.save()` recomputes
//! `description_stripped` on every save (`db/models/page.py:70-77`), so a
//! description update must refresh the stored stripped text.

use api::middleware::auth::AuthUser;
use api::routes::page::desc_patch;
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

async fn state() -> AppState {
    AppState {
        pool: pool().await,
        redis: redis::Client::open("redis://127.0.0.1:6379").expect("redis client"),
        config: AppConfig::from_env(),
    }
}

struct Scratch {
    slug: String,
    workspace_id: Uuid,
    user_id: Uuid,
    project_id: Uuid,
    page_id: Uuid,
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

async fn insert_workspace_member(pool: &PgPool, user_id: Uuid, workspace_id: Uuid, role: i16) {
    sqlx::query(
        "INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, \
         workspace_id, view_props, default_props, issue_props, explored_features, \
         getting_started_checklist, tips, is_active) \
         VALUES (gen_random_uuid(), now(), now(), $1, $2, $3, '{}', '{}', '{}', '{}', '{}', \
         '{}', true)",
    )
    .bind(role)
    .bind(user_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch workspace member");
}

async fn insert_project_member(
    pool: &PgPool,
    user_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    role: i16,
) {
    sqlx::query(
        "INSERT INTO project_members (id, member_id, role, project_id, workspace_id, is_active, \
         view_props, default_props, sort_order, preferences, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, true, '{}', '{}', 65535, '{}', now(), now())",
    )
    .bind(user_id)
    .bind(role)
    .bind(project_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch project member");
}

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let slug = format!("pdsc-{}", Uuid::new_v4().simple());
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let page_id = Uuid::new_v4();
        let identifier =
            format!("PDSC{}", &Uuid::new_v4().simple().to_string()[..8]).to_uppercase();

        insert_user(pool, user_id, &slug).await;

        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'Page Desc Scratch', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
        )
        .bind(workspace_id)
        .bind(&slug)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("scratch workspace");

        insert_workspace_member(pool, user_id, workspace_id, 20).await;

        sqlx::query(
            "INSERT INTO projects (id, created_at, updated_at, name, description, network, \
             identifier, workspace_id, cycle_view, module_view, issue_views_view, page_view, \
             intake_view, archive_in, close_in, logo_props, is_time_tracking_enabled, \
             is_issue_type_enabled, guest_view_all_features, timezone) \
             VALUES ($1, now(), now(), 'Page Desc Scratch', '', 2, $2, $3, false, false, false, \
             false, false, 30, 30, '{}'::jsonb, false, false, false, 'UTC')",
        )
        .bind(project_id)
        .bind(&identifier)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch project");

        insert_project_member(pool, user_id, project_id, workspace_id, 20).await;

        sqlx::query(
            "INSERT INTO pages (id, name, description_json, description_binary, description_html, \
             description_stripped, owned_by_id, created_by_id, updated_by_id, workspace_id, color, \
             access, parent_id, archived_at, is_locked, view_props, logo_props, is_global, \
             sort_order, created_at, updated_at) \
             VALUES ($1, 'Doc', '{}'::jsonb, NULL, '<p>old</p>', 'old', $2, $2, $2, $3, '', 0, \
             NULL, NULL, false, '{}'::jsonb, '{}'::jsonb, false, 65535, now(), now())",
        )
        .bind(page_id)
        .bind(user_id)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch page");

        sqlx::query(
            "INSERT INTO project_pages (id, workspace_id, project_id, page_id, created_by_id, \
             updated_by_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $4, now(), now())",
        )
        .bind(workspace_id)
        .bind(project_id)
        .bind(page_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("scratch project page link");

        Self {
            slug,
            workspace_id,
            user_id,
            project_id,
            page_id,
        }
    }

    async fn cleanup(&self, pool: &PgPool) {
        sqlx::query("DELETE FROM project_pages WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM pages WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM project_members WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM projects WHERE id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(self.user_id)
            .execute(pool)
            .await
            .ok();
    }
}

async fn stripped_of(pool: &PgPool, page_id: Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT description_stripped FROM pages WHERE id = $1")
        .bind(page_id)
        .fetch_one(pool)
        .await
        .expect("page row")
}

#[tokio::test]
async fn desc_patch_recomputes_stripped() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;

    let resp = desc_patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, scratch.page_id)),
        Json(json!({"description_html": "<p>hello <b>world</b></p>"})),
    )
    .await
    .expect("desc_patch");
    assert_eq!(resp.status(), StatusCode::OK);

    let (html, stripped) = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT description_html, description_stripped FROM pages WHERE id = $1",
    )
    .bind(scratch.page_id)
    .fetch_one(&pool)
    .await
    .expect("page row");
    assert_eq!(html, "<p>hello <b>world</b></p>");
    assert_eq!(stripped.as_deref(), Some("hello world"));

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn desc_patch_without_html_keeps_stripped() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;

    let resp = desc_patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, scratch.page_id)),
        Json(json!({"description_binary": ""})),
    )
    .await
    .expect("desc_patch");
    assert_eq!(resp.status(), StatusCode::OK);

    assert_eq!(
        stripped_of(&pool, scratch.page_id).await.as_deref(),
        Some("old")
    );

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn desc_patch_clearing_html_clears_stripped() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;

    let resp = desc_patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, scratch.page_id)),
        Json(json!({"description_html": "<p></p>"})),
    )
    .await
    .expect("desc_patch");
    assert_eq!(resp.status(), StatusCode::OK);

    assert_eq!(stripped_of(&pool, scratch.page_id).await, None);

    scratch.cleanup(&pool).await;
}
