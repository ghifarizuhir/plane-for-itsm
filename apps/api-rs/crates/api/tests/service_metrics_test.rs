//! DB-backed service metrics tests. Run serially:
//! `DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
//!  cargo test -p api --test service_metrics_test -- --test-threads=1`

use api::routes::service_metrics::ingest;
use api::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use chrono::{DateTime, Utc};
use common::config::AppConfig;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

fn database_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://plane:plane@localhost:5432/plane".into())
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
         VALUES (gen_random_uuid(), now(), now(), $1, $2, $3, '{}', '{}', '{}', '{}', '{}', '{}', true)",
    )
    .bind(role)
    .bind(user_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch workspace member");
}

async fn insert_project(pool: &PgPool, project_id: Uuid, workspace_id: Uuid, name: &str, identifier: &str) {
    sqlx::query(
        "INSERT INTO projects (id, created_at, updated_at, name, description, network, \
         identifier, workspace_id, cycle_view, module_view, issue_views_view, page_view, \
         intake_view, archive_in, close_in, logo_props, is_time_tracking_enabled, \
         is_issue_type_enabled, guest_view_all_features, timezone) \
         VALUES ($1, now(), now(), $2, '', 2, $3, $4, false, false, false, false, false, \
         30, 30, '{}'::jsonb, false, false, false, 'UTC')",
    )
    .bind(project_id)
    .bind(name)
    .bind(identifier)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch project");
}

async fn insert_project_member(pool: &PgPool, user_id: Uuid, project_id: Uuid, workspace_id: Uuid, role: i16) {
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

async fn insert_service(pool: &PgPool, workspace_id: Uuid, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO services (id, workspace_id, name, description, description_html, status, \
         criticality, \"type\", sort_order, created_at, updated_at) \
         VALUES ($1, $2, $3, '', '', 'active', 'medium', 'internal', 65535, now(), now())",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(name)
    .execute(pool)
    .await
    .expect("scratch service");
    id
}

async fn link_service(pool: &PgPool, service_id: Uuid, project_id: Uuid, workspace_id: Uuid) {
    sqlx::query(
        "INSERT INTO service_project_links (id, workspace_id, service_id, project_id, link_type, \
         created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, 'owner', now(), now())",
    )
    .bind(workspace_id)
    .bind(service_id)
    .bind(project_id)
    .execute(pool)
    .await
    .expect("scratch service link");
}

async fn insert_source(pool: &PgPool, workspace_id: Uuid, token: &str, config: Value) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO metric_sources (id, workspace_id, name, token, is_active, config, \
         created_at, updated_at) VALUES ($1, $2, $3, $4, true, $5, now(), now())",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(format!("source-{id}"))
    .bind(token)
    .bind(config)
    .execute(pool)
    .await
    .expect("scratch metric source");
    id
}

struct Scratch {
    workspace_id: Uuid,
    admin: Uuid,
    member: Uuid,
    project_id: Uuid,
    service_id: Uuid,
    slug: String,
}

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let workspace_id = Uuid::new_v4();
        let admin = Uuid::new_v4();
        let member = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let slug = format!("metrics-{}", Uuid::new_v4().simple());
        insert_user(pool, admin, &format!("{slug}-admin")).await;
        insert_user(pool, member, &format!("{slug}-member")).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'Metrics Scratch', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
        )
        .bind(workspace_id)
        .bind(&slug)
        .bind(admin)
        .execute(pool)
        .await
        .expect("scratch workspace");
        insert_workspace_member(pool, admin, workspace_id, 20).await;
        insert_workspace_member(pool, member, workspace_id, 15).await;
        insert_project(pool, project_id, workspace_id, "Metrics Project", "MTR").await;
        insert_project_member(pool, member, project_id, workspace_id, 15).await;
        let service_id = insert_service(pool, workspace_id, "Payment API").await;
        link_service(pool, service_id, project_id, workspace_id).await;
        Self { workspace_id, admin, member, project_id, service_id, slug }
    }
}

fn aligned_now(resolution: i64) -> String {
    let now = Utc::now().timestamp();
    let ts = now - now.rem_euclid(resolution);
    DateTime::<Utc>::from_timestamp(ts, 0).unwrap().to_rfc3339()
}

#[tokio::test]
async fn ingest_creates_metric_and_bucket_from_service_map() {
    let pool = pool().await;
    let sc = Scratch::new(&pool).await;
    let token = format!("plane_ms_{}", Uuid::new_v4().simple());
    insert_source(&pool, sc.workspace_id, &token, json!({"service_map": {"payment-api": sc.service_id}})).await;

    let (status, Json(body)) = ingest(
        State(state().await),
        Path(token),
        Json(json!({"metrics": [{
            "service": "payment-api",
            "key": "http.latency",
            "unit": "ms",
            "resolution": 60,
            "bucket_start": aligned_now(60),
            "count": 60, "sum": 7200, "min": 80, "max": 250, "avg": 120, "last": 130
        }]})),
    )
    .await
    .unwrap();

    assert_eq!(status, axum::http::StatusCode::ACCEPTED);
    assert_eq!(body["accepted"], 1);
    assert_eq!(body["metrics_created"], 1);
    let avg: Option<f64> =
        sqlx::query_scalar("SELECT \"avg\" FROM service_metric_buckets WHERE service_id = $1")
            .bind(sc.service_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(avg, Some(120.0));
}

#[tokio::test]
async fn ingest_upserts_same_bucket_last_write_wins() {
    let pool = pool().await;
    let sc = Scratch::new(&pool).await;
    let token = format!("plane_ms_{}", Uuid::new_v4().simple());
    insert_source(&pool, sc.workspace_id, &token, json!({"service_map": {"payment-api": sc.service_id}})).await;
    let bucket = aligned_now(60);
    for avg in [100.0, 200.0] {
        let (status, _) = ingest(
            State(state().await),
            Path(token.clone()),
            Json(json!({"metrics": [{
                "service": "payment-api", "key": "http.latency", "unit": "ms",
                "resolution": 60, "bucket_start": bucket,
                "count": 60, "avg": avg
            }]})),
        )
        .await
        .unwrap();
        assert_eq!(status, axum::http::StatusCode::ACCEPTED);
    }
    let (count, avg): (i64, f64) = sqlx::query_as(
        "SELECT count(*), max(\"avg\") FROM service_metric_buckets WHERE service_id = $1",
    )
    .bind(sc.service_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(avg, 200.0);
}

#[tokio::test]
async fn ingest_batch_is_atomic_on_invalid_item() {
    let pool = pool().await;
    let sc = Scratch::new(&pool).await;
    let token = format!("plane_ms_{}", Uuid::new_v4().simple());
    insert_source(&pool, sc.workspace_id, &token, json!({"service_map": {"payment-api": sc.service_id}})).await;

    let (status, Json(body)) = ingest(
        State(state().await),
        Path(token),
        Json(json!({"metrics": [
            {"service": "payment-api", "key": "ok.metric", "resolution": 60,
             "bucket_start": aligned_now(60), "avg": 1.0},
            {"service": "payment-api", "key": "bad key!", "resolution": 60,
             "bucket_start": aligned_now(60), "avg": 1.0}
        ]})),
    )
    .await
    .unwrap();

    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(body["errors"][0]["index"], 1);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM service_metric_buckets WHERE service_id = $1")
        .bind(sc.service_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}
