//! DB-backed intake source CRUD tests. Run with `--test-threads=1`.

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
    user_id: Uuid,
    project_id: Uuid,
}

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let slug = format!("isrc-{}", Uuid::new_v4().simple());
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        insert_user(pool, user_id, &slug).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'Intake Sources', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
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
             VALUES ($1, now(), now(), 'Intake Sources', '', 2, $2, $3, false, false, false, false, \
             false, 30, 30, '{}'::jsonb, false, false, false, 'UTC')",
        )
        .bind(project_id)
        .bind(format!("ISR{}", &Uuid::new_v4().simple().to_string()[..8]).to_uppercase())
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
        Self {
            slug,
            workspace_id,
            user_id,
            project_id,
        }
    }

    async fn add_intake(&self, pool: &PgPool) {
        sqlx::query(
            "INSERT INTO intakes (id, name, description, is_default, view_props, logo_props, \
             project_id, workspace_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), 'Intake', '', true, '{}'::jsonb, '{}'::jsonb, $1, $2, \
             now(), now())",
        )
        .bind(self.project_id)
        .bind(self.workspace_id)
        .execute(pool)
        .await
        .expect("scratch intake");
    }

    async fn add_type(&self, pool: &PgPool, name: &str) -> Uuid {
        let type_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, \
             is_active, level, requires_service, workspace_id, created_at, updated_at) \
             VALUES ($1, $2, 'Something is broken', '{}'::jsonb, false, false, true, 0, false, $3, \
             now(), now())",
        )
        .bind(type_id)
        .bind(name)
        .bind(self.workspace_id)
        .execute(pool)
        .await
        .expect("scratch type");
        sqlx::query(
            "INSERT INTO project_issue_types (id, created_at, updated_at, project_id, \
             workspace_id, issue_type_id, level, is_default) \
             VALUES (gen_random_uuid(), now(), now(), $1, $2, $3, 0, false)",
        )
        .bind(self.project_id)
        .bind(self.workspace_id)
        .bind(type_id)
        .execute(pool)
        .await
        .expect("scratch project type");
        type_id
    }

    async fn add_service(&self, pool: &PgPool, name: &str) -> Uuid {
        let service_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO services (id, workspace_id, project_id, name, description, description_html, \
             status, criticality, \"type\", created_at, updated_at) \
             VALUES ($1, $2, $3, $4, '', '', 'active', 'high', 'internal', now(), now())",
        )
        .bind(service_id)
        .bind(self.workspace_id)
        .bind(self.project_id)
        .bind(name)
        .execute(pool)
        .await
        .expect("scratch service");
        service_id
    }

    async fn add_source(&self, pool: &PgPool) -> (Uuid, String) {
        let id = Uuid::new_v4();
        let token = format!("plane_is_{}", Uuid::new_v4().simple());
        sqlx::query(
            "INSERT INTO intake_sources (id, project_id, name, token, is_active, auto_accept, config, created_by_id, created_at, updated_at) \
             VALUES ($1, $2, 'Prometheus Prod', $3, true, false, '{}'::jsonb, $4, now(), now())",
        )
        .bind(id)
        .bind(self.project_id)
        .bind(&token)
        .bind(self.user_id)
        .execute(pool)
        .await
        .expect("scratch source");
        (id, token)
    }
}

#[tokio::test]
async fn create_validates_config_and_returns_token() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let service_id = scratch.add_service(&pool, "Payment").await;
    let type_id = scratch.add_type(&pool, "Incident").await;
    let st = state(&pool).await;

    let (status, Json(created)) = api::routes::intake_source::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(api::routes::intake_source::CreateIntakeSource {
            name: "Prometheus Prod".into(),
            type_id: Some(type_id),
            auto_accept: Some(false),
            config: Some(serde_json::json!({
                "service_label_key": "service",
                "service_map": { "payment-api": service_id },
                "severity_label_key": "severity",
                "severity_map": { "critical": "urgent" },
                "default_priority": "none"
            })),
        }),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::CREATED);
    let token = created["token"].as_str().unwrap();
    assert!(token.starts_with("plane_is_"));
    assert_eq!(created["auto_accept"], false);

    // service asing ditolak
    let (bad, Json(_)) = api::routes::intake_source::create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(api::routes::intake_source::CreateIntakeSource {
            name: "Bad".into(),
            type_id: Some(type_id),
            auto_accept: None,
            config: Some(serde_json::json!({
                "service_map": { "x": Uuid::new_v4() }
            })),
        }),
    )
    .await
    .unwrap();
    assert_eq!(bad, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rotate_invalidates_old_token() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let (source_id, old_token) = scratch.add_source(&pool).await;
    let st = state(&pool).await;

    let (status, Json(rotated)) = api::routes::intake_source::rotate(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, source_id)),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    let new_token = rotated["token"].as_str().unwrap();
    assert_ne!(new_token, old_token);

    let (old_alive,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM intake_sources WHERE token = $1 AND deleted_at IS NULL)",
    )
    .bind(&old_token)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!old_alive);
}
