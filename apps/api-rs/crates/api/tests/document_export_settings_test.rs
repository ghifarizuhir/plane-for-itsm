//! DB-backed workspace document-export settings tests. Run with `--test-threads=1`.

use api::middleware::auth::AuthUser;
use api::state::AppState;
use axum::extract::{Path, State};
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

struct Scratch {
    slug: String,
    workspace_id: Uuid,
    admin_id: Uuid,
    member_id: Uuid,
}

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let slug = format!("docx-{}", Uuid::new_v4().simple());
        let admin_id = Uuid::new_v4();
        let member_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        insert_user(pool, admin_id, &format!("{slug}-admin")).await;
        insert_user(pool, member_id, &format!("{slug}-member")).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'Docx', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
        )
        .bind(workspace_id)
        .bind(&slug)
        .bind(admin_id)
        .execute(pool)
        .await
        .expect("scratch workspace");
        for (user_id, role) in [(admin_id, 20), (member_id, 15)] {
            sqlx::query(
                "INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, \
                 workspace_id, view_props, default_props, issue_props, explored_features, \
                 getting_started_checklist, tips, is_active) \
                 VALUES (gen_random_uuid(), now(), now(), $1, $2, $3, '{}', '{}', '{}', '{}', \
                 '{}', '{}', true)",
            )
            .bind(role)
            .bind(user_id)
            .bind(workspace_id)
            .execute(pool)
            .await
            .expect("scratch workspace member");
        }
        Self {
            slug,
            workspace_id,
            admin_id,
            member_id,
        }
    }

    async fn purge(&self, pool: &PgPool) {
        for statement in [
            "DELETE FROM workspace_document_settings WHERE workspace_id = $1",
            "DELETE FROM file_assets WHERE workspace_id = $1",
            "DELETE FROM workspace_members WHERE workspace_id = $1",
        ] {
            sqlx::query(statement)
                .bind(self.workspace_id)
                .execute(pool)
                .await
                .ok();
        }
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
async fn get_returns_defaults_when_no_row() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, Json(body)) = api::routes::document_export::get_settings(
        State(st.clone()),
        AuthUser(scratch.member_id),
        Path(scratch.slug.clone()),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], serde_json::Value::Null);
    assert_eq!(body["header_logo_mode"], "none");
    assert_eq!(body["show_page_numbers"], true);
    assert_eq!(body["show_generated_by"], true);
    assert_eq!(body["header_text_lines"], serde_json::json!([]));
    scratch.purge(&pool).await;
}

#[tokio::test]
async fn patch_is_admin_only() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, _) = api::routes::document_export::update_settings(
        State(st.clone()),
        AuthUser(scratch.member_id),
        Path(scratch.slug.clone()),
        Json(api::routes::document_export::UpdateDocumentExportSettings {
            header_logo_mode: Some("workspace".into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::FORBIDDEN);
    scratch.purge(&pool).await;
}

#[tokio::test]
async fn patch_upserts_and_validates() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, Json(body)) = api::routes::document_export::update_settings(
        State(st.clone()),
        AuthUser(scratch.admin_id),
        Path(scratch.slug.clone()),
        Json(api::routes::document_export::UpdateDocumentExportSettings {
            header_logo_mode: Some("workspace".into()),
            header_text_lines: Some(serde_json::json!([
                {"text": "PT Example — IT Services", "align": "center"}
            ])),
            show_generated_by: Some(false),
            ..Default::default()
        }),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["header_logo_mode"], "workspace");
    assert_eq!(body["show_generated_by"], false);
    assert_eq!(body["header_text_lines"][0]["align"], "center");

    let (status, Json(body)) = api::routes::document_export::update_settings(
        State(st.clone()),
        AuthUser(scratch.admin_id),
        Path(scratch.slug.clone()),
        Json(api::routes::document_export::UpdateDocumentExportSettings {
            footer_text_lines: Some(serde_json::json!([{"text": "Confidential"}])),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["footer_text_lines"][0]["text"], "Confidential");
    assert_eq!(body["header_logo_mode"], "workspace");

    let (status, _) = api::routes::document_export::update_settings(
        State(st.clone()),
        AuthUser(scratch.admin_id),
        Path(scratch.slug.clone()),
        Json(api::routes::document_export::UpdateDocumentExportSettings {
            footer_text_lines: Some(serde_json::json!([{"text": "x", "align": "diagonal"}])),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM workspace_document_settings WHERE workspace_id = $1 AND deleted_at IS NULL",
    )
    .bind(scratch.workspace_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    scratch.purge(&pool).await;
}
