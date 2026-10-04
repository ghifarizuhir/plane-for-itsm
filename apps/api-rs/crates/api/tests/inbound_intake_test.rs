//! DB-backed Alertmanager intake ingest tests. Run with `--test-threads=1`.

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
        let slug = format!("inb-{}", Uuid::new_v4().simple());
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        insert_user(pool, user_id, &slug).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'Inbound', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
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
             VALUES ($1, now(), now(), 'Inbound', '', 2, $2, $3, false, false, false, false, \
             false, 30, 30, '{}'::jsonb, false, false, false, 'UTC')",
        )
        .bind(project_id)
        .bind(format!("INB{}", &Uuid::new_v4().simple().to_string()[..8]).to_uppercase())
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

    async fn add_type_requiring_service(&self, pool: &PgPool, name: &str) -> Uuid {
        let type_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, \
             is_active, level, requires_service, workspace_id, created_at, updated_at) \
             VALUES ($1, $2, 'Something is broken', '{}'::jsonb, false, false, true, 0, true, $3, \
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

    async fn add_default_state(&self, pool: &PgPool) {
        sqlx::query(
            "INSERT INTO states (id, name, description, slug, \"group\", color, sequence, is_triage, \
             \"default\", project_id, workspace_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), 'Backlog', '', 'backlog', 'backlog', '#4E5355', 1000, false, \
             true, $1, $2, now(), now())",
        )
        .bind(self.project_id)
        .bind(self.workspace_id)
        .execute(pool)
        .await
        .expect("scratch default state");
    }

    async fn add_completed_state(&self, pool: &PgPool) -> Uuid {
        let state_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO states (id, name, description, slug, \"group\", color, sequence, is_triage, \
             \"default\", project_id, workspace_id, created_at, updated_at) \
             VALUES ($1, 'Done', '', 'done', 'completed', '#4E5355', 2000, false, false, $2, $3, now(), now())",
        )
        .bind(state_id)
        .bind(self.project_id)
        .bind(self.workspace_id)
        .execute(pool)
        .await
        .expect("scratch completed state");
        state_id
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

    async fn add_source_with_config(
        &self,
        pool: &PgPool,
        type_id: Uuid,
        config: serde_json::Value,
    ) -> (Uuid, String) {
        let id = Uuid::new_v4();
        let token = format!("plane_is_{}", Uuid::new_v4().simple());
        sqlx::query(
            "INSERT INTO intake_sources (id, project_id, name, token, is_active, auto_accept, type_id, \
             config, created_by_id, created_at, updated_at) \
             VALUES ($1, $2, 'Prometheus Prod', $3, true, false, $4, $5, $6, now(), now())",
        )
        .bind(id)
        .bind(self.project_id)
        .bind(&token)
        .bind(type_id)
        .bind(&config)
        .bind(self.user_id)
        .execute(pool)
        .await
        .expect("scratch source");
        (id, token)
    }
}

fn alert_payload(fingerprint: &str, status: &str, severity: &str) -> serde_json::Value {
    serde_json::json!({
        "version": "4",
        "status": status,
        "commonLabels": { "severity": severity },
        "alerts": [{
            "status": status,
            "labels": {
                "alertname": "HighErrorRate",
                "service": "payment-api",
                "severity": severity
            },
            "annotations": {
                "summary": "Error rate > 5%",
                "description": "5xx spike on payment-api"
            },
            "startsAt": "2026-10-04T00:00:00Z",
            "endsAt": "0001-01-01T00:00:00Z",
            "generatorURL": "http://prometheus.local/graph",
            "fingerprint": fingerprint
        }]
    })
}

#[tokio::test]
async fn creates_pending_item_with_mapped_type_service_priority() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let service_id = scratch.add_service(&pool, "Payment API").await;
    let type_id = scratch.add_type_requiring_service(&pool, "Incident").await;
    let (source_id, token) = scratch
        .add_source_with_config(
            &pool,
            type_id,
            serde_json::json!({
                "service_label_key": "service",
                "service_map": { "payment-api": service_id },
                "severity_label_key": "severity",
                "severity_map": { "critical": "urgent" },
                "default_priority": "none"
            }),
        )
        .await;
    let st = state(&pool).await;

    let (status, Json(body)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token),
        Json(alert_payload("fp-1", "firing", "critical")),
    )
    .await
    .unwrap();

    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["created"], 1);

    let row: (String, String, Uuid, i32, i32, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT i.name, i.priority, i.type_id, ii.status, i.intake_occurrence_count, i.intake_last_seen_at \
         FROM issues i JOIN intake_issues ii ON ii.issue_id = i.id \
         WHERE i.intake_source_id = $1 AND i.intake_fingerprint = 'fp-1'",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "Error rate > 5%");
    assert_eq!(row.1, "urgent");
    assert_eq!(row.2, type_id);
    assert_eq!(row.3, -2);
    assert_eq!(row.4, 1);
    assert!(row.5.is_some());

    let (linked,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM service_issues si JOIN issues i ON i.id = si.issue_id \
         WHERE i.intake_fingerprint = 'fp-1' AND si.service_id = $1 AND si.deleted_at IS NULL)",
    )
    .bind(service_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(linked);

    // token salah → 404
    let (not_found, _) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path("plane_is_nope".to_string()),
        Json(alert_payload("fp-2", "firing", "critical")),
    )
    .await
    .unwrap();
    assert_eq!(not_found, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn firing_upserts_series_and_reopens_declined() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let service_id = scratch.add_service(&pool, "Payment API").await;
    let type_id = scratch.add_type_requiring_service(&pool, "Incident").await;
    scratch.add_default_state(&pool).await;
    let (source_id, token) = scratch
        .add_source_with_config(
            &pool,
            type_id,
            serde_json::json!({ "service_map": { "payment-api": service_id } }),
        )
        .await;
    let st = state(&pool).await;

    for _ in 0..2 {
        let (status, _) = api::routes::inbound::alertmanager(
            State(st.clone()),
            Path(token.clone()),
            Json(alert_payload("fp-dup", "firing", "warning")),
        )
        .await
        .unwrap();
        assert_eq!(status, StatusCode::ACCEPTED);
    }

    let (count, occurrence, row_status): (i64, i32, i32) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM issues WHERE intake_source_id = $1 AND deleted_at IS NULL), \
                i.intake_occurrence_count, ii.status \
         FROM issues i JOIN intake_issues ii ON ii.issue_id = i.id \
         WHERE i.intake_source_id = $1 AND i.intake_fingerprint = 'fp-dup'",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(occurrence, 2);
    assert_eq!(row_status, -2);

    // Triager decline → firing berikutnya membuka lagi.
    sqlx::query(
        "UPDATE intake_issues SET status = -1 WHERE issue_id IN \
         (SELECT id FROM issues WHERE intake_source_id = $1 AND intake_fingerprint = 'fp-dup')",
    )
    .bind(source_id)
    .execute(&pool)
    .await
    .unwrap();
    let (_, Json(body)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(alert_payload("fp-dup", "firing", "warning")),
    )
    .await
    .unwrap();
    assert_eq!(body["reopened"], 1);
    let (status_again,): (i32,) = sqlx::query_as(
        "SELECT ii.status FROM issues i JOIN intake_issues ii ON ii.issue_id = i.id \
         WHERE i.intake_source_id = $1 AND i.intake_fingerprint = 'fp-dup'",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status_again, -2);
}

#[tokio::test]
async fn resolved_declines_pending_and_completes_accepted() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let service_id = scratch.add_service(&pool, "Payment API").await;
    let type_id = scratch.add_type_requiring_service(&pool, "Incident").await;
    scratch.add_default_state(&pool).await;
    scratch.add_completed_state(&pool).await;
    let (source_id, token) = scratch
        .add_source_with_config(
            &pool,
            type_id,
            serde_json::json!({ "service_map": { "payment-api": service_id } }),
        )
        .await;
    let st = state(&pool).await;

    // Pending → resolved = declined.
    let _ = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(alert_payload("fp-res", "firing", "warning")),
    )
    .await
    .unwrap();
    let (_, Json(body)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(alert_payload("fp-res", "resolved", "warning")),
    )
    .await
    .unwrap();
    assert_eq!(body["declined"], 1);
    let (declined,): (i32,) = sqlx::query_as(
        "SELECT ii.status FROM issues i JOIN intake_issues ii ON ii.issue_id = i.id \
         WHERE i.intake_source_id = $1 AND i.intake_fingerprint = 'fp-res'",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(declined, -1);

    // Resolved duplikat = ignored.
    let (_, Json(again)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(alert_payload("fp-res", "resolved", "warning")),
    )
    .await
    .unwrap();
    assert_eq!(again["ignored"], 1);

    // Accepted (manual) + resolved = completed.
    let _ = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(alert_payload("fp-acc", "firing", "warning")),
    )
    .await
    .unwrap();
    let completed_state: Uuid = sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND \"group\" = 'completed' LIMIT 1",
    )
    .bind(scratch.project_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let default_state: Uuid = sqlx::query_scalar(
        "SELECT id FROM states WHERE project_id = $1 AND \"group\" = 'backlog' LIMIT 1",
    )
    .bind(scratch.project_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE intake_issues SET status = 1 WHERE issue_id IN \
         (SELECT id FROM issues WHERE intake_fingerprint = 'fp-acc')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (_, Json(resolved)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(alert_payload("fp-acc", "resolved", "warning")),
    )
    .await
    .unwrap();
    assert_eq!(resolved["resolved"], 1);
    let (state_id,): (Option<Uuid>,) = sqlx::query_as(
        "SELECT state_id FROM issues WHERE intake_source_id = $1 AND intake_fingerprint = 'fp-acc'",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state_id, Some(completed_state));

    // Refire setelah completed = reopen ke state default.
    let (_, Json(reopened)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(alert_payload("fp-acc", "firing", "warning")),
    )
    .await
    .unwrap();
    assert_eq!(reopened["reopened"], 1);
    let (state_id, completed_at): (Option<Uuid>, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT state_id, completed_at FROM issues WHERE intake_source_id = $1 AND intake_fingerprint = 'fp-acc'")
            .bind(source_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state_id, Some(default_state));
    assert!(completed_at.is_none());
}

#[tokio::test]
async fn auto_accept_only_when_classification_complete() {
    let pool = pool().await;
    // Bersihkan sisa run sebelumnya agar fetch tanpa scope tidak salah baca.
    sqlx::query(
        "UPDATE intake_issues SET deleted_at = now() WHERE issue_id IN \
         (SELECT id FROM issues WHERE intake_fingerprint IN ('fp-auto', 'fp-nosvc'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE issues SET deleted_at = now() WHERE intake_fingerprint IN ('fp-auto', 'fp-nosvc')")
        .execute(&pool)
        .await
        .unwrap();
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let service_id = scratch.add_service(&pool, "Payment API").await;
    let type_id = scratch.add_type_requiring_service(&pool, "Incident").await;
    scratch.add_default_state(&pool).await;
    let (_, token) = scratch
        .add_source_with_config(
            &pool,
            type_id,
            serde_json::json!({ "service_map": { "payment-api": service_id } }),
        )
        .await;
    // Nyalakan auto_accept.
    sqlx::query("UPDATE intake_sources SET auto_accept = true WHERE token = $1")
        .bind(&token)
        .execute(&pool)
        .await
        .unwrap();
    let source_id: Uuid = sqlx::query_scalar("SELECT id FROM intake_sources WHERE token = $1")
        .bind(&token)
        .fetch_one(&pool)
        .await
        .unwrap();
    let st = state(&pool).await;

    let (_, Json(body)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(alert_payload("fp-auto", "firing", "critical")),
    )
    .await
    .unwrap();
    assert_eq!(body["accepted"], 1);
    let (status, state_group): (i32, Option<String>) = sqlx::query_as(
        "SELECT ii.status, st.\"group\" FROM issues i \
         JOIN intake_issues ii ON ii.issue_id = i.id \
         LEFT JOIN states st ON st.id = i.state_id \
         WHERE i.intake_source_id = $1 AND i.intake_fingerprint = 'fp-auto'",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, 1);
    assert_ne!(state_group.as_deref(), Some("triage"));

    // Source tanpa mapping service → tetap pending (Incident requires_service).
    let (_, token_no_service) = scratch
        .add_source_with_config(&pool, type_id, serde_json::json!({}))
        .await;
    sqlx::query("UPDATE intake_sources SET auto_accept = true WHERE token = $1")
        .bind(&token_no_service)
        .execute(&pool)
        .await
        .unwrap();
    let (_, Json(body)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token_no_service.clone()),
        Json(alert_payload("fp-nosvc", "firing", "critical")),
    )
    .await
    .unwrap();
    assert_eq!(body["accepted"], 0);
    assert_eq!(body["created"], 1);
    let (status,): (i32,) = sqlx::query_as(
        "SELECT ii.status FROM issues i JOIN intake_issues ii ON ii.issue_id = i.id \
         WHERE i.intake_fingerprint = 'fp-nosvc' AND i.deleted_at IS NULL \
         ORDER BY i.created_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, -2);
}

#[tokio::test]
async fn batch_common_labels_and_fingerprint_fallback() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let service_id = scratch.add_service(&pool, "Payment API").await;
    let type_id = scratch.add_type_requiring_service(&pool, "Incident").await;
    scratch.add_default_state(&pool).await;
    scratch.add_completed_state(&pool).await;
    let (source_id, token) = scratch
        .add_source_with_config(
            &pool,
            type_id,
            serde_json::json!({
                "service_map": { "payment-api": service_id },
                "severity_map": { "critical": "urgent" },
                "default_priority": "none"
            }),
        )
        .await;
    let st = state(&pool).await;

    // Dua alert tanpa fingerprint; severity hanya di commonLabels.
    let payload = serde_json::json!({
        "version": "4",
        "commonLabels": { "severity": "critical", "service": "payment-api" },
        "alerts": [
            { "status": "firing", "labels": { "alertname": "A" }, "annotations": { "summary": "A down" } },
            { "status": "firing", "labels": { "alertname": "B" }, "annotations": { "summary": "B down" } }
        ]
    });
    let (status, Json(body)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(payload.clone()),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["created"], 2);

    // Kirim ulang payload yang sama: idempoten (update, bukan duplikat).
    let (_, Json(second)) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(payload.clone()),
    )
    .await
    .unwrap();
    assert_eq!(second["created"], 0);
    assert_eq!(second["updated"], 2);
    let (total,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM issues WHERE intake_source_id = $1 AND deleted_at IS NULL",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(total, 2);
    let (urgent_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM issues WHERE intake_source_id = $1 AND priority = 'urgent' AND deleted_at IS NULL",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(urgent_count, 2);

    // Body invalid → 400.
    let (bad, _) = api::routes::inbound::alertmanager(
        State(st.clone()),
        Path(token.clone()),
        Json(serde_json::json!({ "alerts": "nope" })),
    )
    .await
    .unwrap();
    assert_eq!(bad, StatusCode::BAD_REQUEST);
}
