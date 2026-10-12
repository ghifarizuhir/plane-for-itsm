//! `?parent=` filter on the pages list (used by the DOCX exporter to
//! enumerate sub-pages). Run with `--test-threads=1`.

use api::middleware::auth::AuthUser;
use api::routes::page::{list, PageListQuery};
use api::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use common::config::AppConfig;
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
    project_id: Uuid,
    parent_id: Uuid,
    child_id: Uuid,
}

async fn insert_page(
    pool: &PgPool,
    page_id: Uuid,
    name: &str,
    user_id: Uuid,
    workspace_id: Uuid,
    parent_id: Option<Uuid>,
    project_id: Uuid,
) {
    sqlx::query(
        "INSERT INTO pages (id, name, description_json, description_binary, description_html, \
         description_stripped, owned_by_id, created_by_id, updated_by_id, workspace_id, color, \
         access, parent_id, archived_at, is_locked, view_props, logo_props, is_global, \
         sort_order, created_at, updated_at) \
         VALUES ($1, $2, '{}'::jsonb, NULL, '<p>x</p>', 'x', $3, $3, $3, $4, '', 0, \
         $5, NULL, false, '{}'::jsonb, '{}'::jsonb, false, 65535, now(), now())",
    )
    .bind(page_id)
    .bind(name)
    .bind(user_id)
    .bind(workspace_id)
    .bind(parent_id)
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
}

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let slug = format!("ppar-{}", Uuid::new_v4().simple());
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();
        let identifier = format!("PPAR{}", &Uuid::new_v4().simple().to_string()[..8]).to_uppercase();

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
        .bind(&slug)
        .bind(format!("{slug}@example.invalid"))
        .bind(Uuid::new_v4().simple().to_string())
        .execute(pool)
        .await
        .expect("scratch user");

        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'Page Parent Scratch', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
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

        sqlx::query(
            "INSERT INTO projects (id, created_at, updated_at, name, description, network, \
             identifier, workspace_id, cycle_view, module_view, issue_views_view, page_view, \
             intake_view, archive_in, close_in, logo_props, is_time_tracking_enabled, \
             is_issue_type_enabled, guest_view_all_features, timezone) \
             VALUES ($1, now(), now(), 'Page Parent Scratch', '', 2, $2, $3, false, false, false, \
             false, false, 30, 30, '{}'::jsonb, false, false, false, 'UTC')",
        )
        .bind(project_id)
        .bind(&identifier)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch project");

        sqlx::query(
            "INSERT INTO project_members (id, member_id, role, project_id, workspace_id, is_active, \
             view_props, default_props, sort_order, preferences, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, 20, $2, $3, true, '{}', '{}', 65535, '{}', now(), now())",
        )
        .bind(user_id)
        .bind(project_id)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch project member");

        insert_page(pool, parent_id, "Parent", user_id, workspace_id, None, project_id).await;
        insert_page(pool, child_id, "Child", user_id, workspace_id, Some(parent_id), project_id).await;

        Self {
            slug,
            workspace_id,
            user_id,
            project_id,
            parent_id,
            child_id,
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

fn names(body: &serde_json::Value) -> Vec<String> {
    body.as_array()
        .expect("list returns an array")
        .iter()
        .map(|row| row["name"].as_str().unwrap_or("").to_string())
        .collect()
}

#[tokio::test]
async fn list_defaults_to_top_level_pages() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, Json(body)) = list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Query(PageListQuery::default()),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(names(&body), vec!["Parent".to_string()]);
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn list_with_parent_returns_children() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, Json(body)) = list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Query(PageListQuery {
            parent: Some(scratch.parent_id),
            ..Default::default()
        }),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(names(&body), vec!["Child".to_string()]);
    assert_eq!(
        body[0]["parent"].as_str(),
        Some(scratch.parent_id.to_string()).as_deref()
    );
    scratch.cleanup(&pool).await;
}
