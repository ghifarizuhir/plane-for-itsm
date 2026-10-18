//! Workspace-level feature settings (program P1/S2). Run with `--test-threads=1`.

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
         last_login_medium, last_logout_uagent, is_bot, display_name, is_email_valid, \
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
    guest_id: Uuid,
}

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let slug = format!("wsf-{}", Uuid::new_v4().simple());
        let admin_id = Uuid::new_v4();
        let member_id = Uuid::new_v4();
        let guest_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        insert_user(pool, admin_id, &format!("{slug}-admin")).await;
        insert_user(pool, member_id, &format!("{slug}-member")).await;
        insert_user(pool, guest_id, &format!("{slug}-guest")).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'WSF', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
        )
        .bind(workspace_id)
        .bind(&slug)
        .bind(admin_id)
        .execute(pool)
        .await
        .expect("scratch workspace");
        for (user_id, role) in [(admin_id, 20), (member_id, 15), (guest_id, 5)] {
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
            guest_id,
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_project(
        &self,
        pool: &PgPool,
        name: &str,
        identifier: &str,
        cycle_view: bool,
        module_view: bool,
        intake_view: bool,
        archive_in: i32,
        close_in: i32,
        guest_view_all: bool,
    ) -> Uuid {
        let project_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO projects (id, name, description, identifier, workspace_id, network, \
             module_view, cycle_view, issue_views_view, page_view, intake_view, \
             is_time_tracking_enabled, is_issue_type_enabled, guest_view_all_features, \
             archive_in, close_in, logo_props, timezone, created_by_id, updated_by_id, \
             created_at, updated_at) \
             VALUES ($1, $2, '', $3, $4, 2, $5, $6, true, true, $7, false, false, $8, $9, \
             $10, '{}', 'UTC', $11, $11, now(), now())",
        )
        .bind(project_id)
        .bind(name)
        .bind(identifier)
        .bind(self.workspace_id)
        .bind(module_view)
        .bind(cycle_view)
        .bind(intake_view)
        .bind(guest_view_all)
        .bind(archive_in)
        .bind(close_in)
        .bind(self.admin_id)
        .execute(pool)
        .await
        .expect("scratch project");
        project_id
    }

    async fn purge(&self, pool: &PgPool) {
        for statement in [
            "DELETE FROM workspace_settings WHERE workspace_id = $1",
            "DELETE FROM project_members WHERE workspace_id = $1",
            "DELETE FROM projects WHERE workspace_id = $1",
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
async fn backfill_union_across_projects() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    // Project A: cycles/modules/intake on, archive rule, member as default assignee.
    scratch
        .insert_project(&pool, "App A", "APPA", true, true, true, 3, 0, false)
        .await;
    // Project B: everything off except close rule + guest flag.
    scratch
        .insert_project(&pool, "App B", "APPB", false, false, false, 0, 6, true)
        .await;
    sqlx::query(
        "UPDATE projects SET default_assignee_id = $1 WHERE workspace_id = $2 AND identifier = 'APPA'",
    )
    .bind(scratch.member_id)
    .bind(scratch.workspace_id)
    .execute(&pool)
    .await
    .expect("set default assignee");

    let (status, Json(body)) = api::routes::workspace_settings::get_settings(
        State(st.clone()),
        AuthUser(scratch.admin_id),
        Path(scratch.slug.clone()),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cycle_view"], true);
    assert_eq!(body["module_view"], true);
    assert_eq!(body["intake_view"], true);
    assert_eq!(body["guest_view_all_features"], true);
    assert_eq!(body["archive_in"], 3);
    assert_eq!(body["close_in"], 6);
    assert_eq!(
        body["default_assignee_id"],
        serde_json::json!(scratch.member_id.to_string())
    );
    scratch.purge(&pool).await;
}

#[tokio::test]
async fn get_creates_row_when_missing() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    scratch
        .insert_project(&pool, "Solo", "SOLO", true, false, false, 0, 0, false)
        .await;

    let (status, Json(body)) = api::routes::workspace_settings::get_settings(
        State(st.clone()),
        AuthUser(scratch.member_id),
        Path(scratch.slug.clone()),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cycle_view"], true);
    assert_eq!(body["module_view"], false);
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM workspace_settings WHERE workspace_id = $1 AND deleted_at IS NULL",
    )
    .bind(scratch.workspace_id)
    .fetch_one(&pool)
    .await
    .expect("count settings rows");
    assert_eq!(count, 1);
    scratch.purge(&pool).await;
}

#[tokio::test]
async fn get_forbidden_for_non_member() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let outsider_id = Uuid::new_v4();
    insert_user(&pool, outsider_id, &format!("{}-outsider", scratch.slug)).await;

    let (status, _) = api::routes::workspace_settings::get_settings(
        State(st.clone()),
        AuthUser(outsider_id),
        Path(scratch.slug.clone()),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::FORBIDDEN);
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(outsider_id)
        .execute(&pool)
        .await
        .ok();
    scratch.purge(&pool).await;
}

#[tokio::test]
async fn patch_admin_only() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    for user in [scratch.member_id, scratch.guest_id] {
        let (status, _) = api::routes::workspace_settings::update_settings(
            State(st.clone()),
            AuthUser(user),
            Path(scratch.slug.clone()),
            Json(api::routes::workspace_settings::UpdateWorkspaceSettings {
                cycle_view: Some(false),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    let (status, Json(body)) = api::routes::workspace_settings::update_settings(
        State(st.clone()),
        AuthUser(scratch.admin_id),
        Path(scratch.slug.clone()),
        Json(api::routes::workspace_settings::UpdateWorkspaceSettings {
            cycle_view: Some(false),
            ..Default::default()
        }),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cycle_view"], false);
    // Untouched flags keep union/default values.
    assert_eq!(body["module_view"], true);
    scratch.purge(&pool).await;
}

#[tokio::test]
async fn patch_validates() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, _) = api::routes::workspace_settings::update_settings(
        State(st.clone()),
        AuthUser(scratch.admin_id),
        Path(scratch.slug.clone()),
        Json(api::routes::workspace_settings::UpdateWorkspaceSettings {
            archive_in: Some(-1),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = api::routes::workspace_settings::update_settings(
        State(st.clone()),
        AuthUser(scratch.admin_id),
        Path(scratch.slug.clone()),
        Json(api::routes::workspace_settings::UpdateWorkspaceSettings {
            default_assignee_id: Some(Uuid::new_v4()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, Json(body)) = api::routes::workspace_settings::update_settings(
        State(st.clone()),
        AuthUser(scratch.admin_id),
        Path(scratch.slug.clone()),
        Json(api::routes::workspace_settings::UpdateWorkspaceSettings {
            default_assignee_id: Some(scratch.member_id),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["default_assignee_id"],
        serde_json::json!(scratch.member_id.to_string())
    );
    scratch.purge(&pool).await;
}

#[tokio::test]
async fn patch_partial_preserves_rest() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    scratch
        .insert_project(&pool, "Solo", "SOLO", true, true, false, 0, 0, false)
        .await;

    let (status, Json(body)) = api::routes::workspace_settings::update_settings(
        State(st.clone()),
        AuthUser(scratch.admin_id),
        Path(scratch.slug.clone()),
        Json(api::routes::workspace_settings::UpdateWorkspaceSettings {
            cycle_view: Some(false),
            ..Default::default()
        }),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cycle_view"], false);
    assert_eq!(body["module_view"], true);
    assert_eq!(body["intake_view"], false);
    scratch.purge(&pool).await;
}
