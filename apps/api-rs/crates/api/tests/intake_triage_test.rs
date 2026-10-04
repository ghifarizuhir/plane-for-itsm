//! DB-backed Jev intake triage tests. These tests mutate the process env, so
//! run with `--test-threads=1`.

mod support;

use api::middleware::auth::AuthUser;
use api::routes::intake::{
    apply_triage_suggestion, dismiss_triage_suggestion, get_triage_suggestion, patch_issue,
    CreateIntakeIssue, InboxIssueFields, InboxIssuePatch, IntakeIssuePayload, TriageFieldsBody,
};
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
        let slug = format!("trg-{}", Uuid::new_v4().simple());
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        insert_user(pool, user_id, &slug).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'Triage', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
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
             VALUES ($1, now(), now(), 'Triage', '', 2, $2, $3, false, false, false, false, \
             false, 30, 30, '{}'::jsonb, false, false, false, 'UTC')",
        )
        .bind(project_id)
        .bind(format!("TRG{}", &Uuid::new_v4().simple().to_string()[..8]).to_uppercase())
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

    async fn link_service(&self, pool: &PgPool, service_id: Uuid, issue_id: Uuid) {
        sqlx::query(
            "INSERT INTO service_issues (id, workspace_id, project_id, service_id, issue_id, \
             created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now())",
        )
        .bind(self.workspace_id)
        .bind(self.project_id)
        .bind(service_id)
        .bind(issue_id)
        .execute(pool)
        .await
        .expect("scratch service link");
    }

    async fn create_item(&self, state: &AppState, name: &str) -> (Uuid, Uuid) {
        let (status, Json(detail)) = api::routes::intake::create_issue(
            State(state.clone()),
            AuthUser(self.user_id),
            Path((self.slug.clone(), self.project_id)),
            Json(CreateIntakeIssue {
                issue: IntakeIssuePayload {
                    name: Some(name.to_string()),
                    priority: None,
                },
            }),
        )
        .await
        .expect("intake create");
        assert_eq!(status, StatusCode::OK);
        let row_id = Uuid::parse_str(detail["id"].as_str().unwrap()).unwrap();
        let issue_id = Uuid::parse_str(detail["issue"]["id"].as_str().unwrap()).unwrap();
        (row_id, issue_id)
    }

    async fn purge(&self, pool: &PgPool) {
        for statement in [
            "DELETE FROM intake_triage_suggestions WHERE workspace_id = $1",
            "DELETE FROM intake_issues WHERE workspace_id = $1",
            "DELETE FROM issue_sequences WHERE project_id = $1",
            "DELETE FROM issue_description_versions WHERE project_id = $1",
            "DELETE FROM issues WHERE workspace_id = $1",
            "DELETE FROM states WHERE workspace_id = $1",
            "DELETE FROM service_issues WHERE project_id = $1",
            "DELETE FROM services WHERE project_id = $1",
            "DELETE FROM intakes WHERE workspace_id = $1",
            "DELETE FROM project_issue_types WHERE workspace_id = $1",
            "DELETE FROM issue_types WHERE workspace_id = $1",
            "DELETE FROM project_members WHERE workspace_id = $1",
            "DELETE FROM workspace_members WHERE workspace_id = $1",
        ] {
            let scope = if statement.contains("project_id") {
                self.project_id
            } else {
                self.workspace_id
            };
            sqlx::query(statement).bind(scope).execute(pool).await.ok();
        }
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
        sqlx::query("DELETE FROM users WHERE username LIKE $1")
            .bind(format!("{}%", self.slug))
            .execute(pool)
            .await
            .ok();
    }
}

fn set_decision_env(base_url: &str) {
    std::env::set_var("SKIP_ENV_VAR", "0");
    std::env::set_var("LLM_API_KEY", "test-key");
    std::env::set_var("LLM_BASE_URL", base_url);
    std::env::set_var("LLM_DECISION_MODEL", "typesafe/jev-1.13");
}

fn clear_decision_env() {
    std::env::remove_var("SKIP_ENV_VAR");
    std::env::remove_var("LLM_API_KEY");
    std::env::remove_var("LLM_BASE_URL");
    std::env::remove_var("LLM_DECISION_MODEL");
}

#[tokio::test]
async fn classify_stores_suggestion_and_apply_dismiss_work() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    scratch.add_type(&pool, "Incident").await;
    let st = state(&pool).await;

    let (base_url, bodies) = support::spawn_systemone_upstream(json!({
        "model": "jev-1.13.0",
        "answers": {
            "category": {"type": "choice", "choice": "Incident", "confidence": 0.87,
                         "probabilities": {"Incident": 0.87, "Problem": 0.13}},
            "severity": {"type": "score", "score": 3.2, "confidence": 0.91,
                         "legend": {"0": "none", "1": "low", "2": "medium", "3": "high", "4": "urgent"},
                         "probabilities": {"3": 0.85, "4": 0.15}},
            "needs_human": {"type": "noul", "noul": 0.78}
        },
        "usage": {"input_tokens": 120, "output_tokens": 12}
    }))
    .await;
    set_decision_env(&base_url);

    let (row_id, issue_id) = scratch.create_item(&st, "Login is broken").await;
    ai::triage_job::classify(&pool, row_id).await.expect("classify");

    // Request terkirim ke `{base}/systemone` dengan pertanyaan bertipe.
    let sent = bodies.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["model"], "typesafe/jev-1.13");
    assert!(sent[0]["questions"]["category"]["criteria"].get("Incident").is_some());
    assert_eq!(
        sent[0]["questions"]["severity"]["criteria"][3],
        "Major functionality blocked for users"
    );

    let (status, Json(body)) = get_triage_suggestion(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
    )
    .await
    .expect("get suggestion");
    assert_eq!(status, StatusCode::OK);
    let suggestion = &body["data"];
    assert_eq!(suggestion["status"], "ready");
    assert_eq!(suggestion["model"], "jev-1.13.0");
    assert_eq!(suggestion["category"]["label"], "Incident");
    assert_eq!(suggestion["severity"]["priority"], "high");
    assert_eq!(suggestion["severity"]["probabilities"]["high"], 0.85);
    assert_eq!(suggestion["needs_human"]["probability"], 0.78);

    let (status, Json(applied)) = apply_triage_suggestion(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
        Json(TriageFieldsBody {
            fields: vec!["severity".to_string()],
        }),
    )
    .await
    .expect("apply");
    assert_eq!(status, StatusCode::OK);
    assert!(applied["data"]["applied_fields"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "severity"));
    let priority: String = sqlx::query_scalar("SELECT priority FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(priority, "high");

    let (status, Json(dismissed)) = dismiss_triage_suggestion(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
        Json(TriageFieldsBody {
            fields: vec!["category".to_string(), "needs_human".to_string()],
        }),
    )
    .await
    .expect("dismiss");
    assert_eq!(status, StatusCode::OK);
    let dismissed_fields = dismissed["data"]["dismissed_fields"].as_array().unwrap();
    assert!(dismissed_fields.iter().any(|field| field == "category"));
    assert!(dismissed_fields.iter().any(|field| field == "needs_human"));

    // Field dismissed tidak bisa di-apply.
    let (status, _) = apply_triage_suggestion(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
        Json(TriageFieldsBody {
            fields: vec!["category".to_string()],
        }),
    )
    .await
    .expect("apply dismissed");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    clear_decision_env();
    scratch.purge(&pool).await;
}

#[tokio::test]
async fn sweep_candidates_skips_ready_and_recent_failures() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let st = state(&pool).await;

    let (ready_id, _) = scratch.create_item(&st, "already classified").await;
    let (recent_failed, _) = scratch.create_item(&st, "failed recently").await;
    let (old_failed, _) = scratch.create_item(&st, "failed long ago").await;
    let (unclassified, _) = scratch.create_item(&st, "never classified").await;

    sqlx::query(
        "INSERT INTO intake_triage_suggestions (id, intake_issue_id, project_id, workspace_id, \
         status, created_at, updated_at) VALUES \
         (gen_random_uuid(), $1, $2, $3, 'ready', now(), now()), \
         (gen_random_uuid(), $4, $2, $3, 'failed', now(), now()), \
         (gen_random_uuid(), $5, $2, $3, 'failed', now(), now() - interval '10 minutes')",
    )
    .bind(ready_id)
    .bind(scratch.project_id)
    .bind(scratch.workspace_id)
    .bind(recent_failed)
    .bind(old_failed)
    .execute(&pool)
    .await
    .expect("seed suggestions");

    let candidates = ai::triage_job::sweep_candidates(&pool, 10).await.unwrap();
    assert!(candidates.contains(&unclassified));
    assert!(candidates.contains(&old_failed));
    assert!(!candidates.contains(&ready_id));
    assert!(!candidates.contains(&recent_failed));

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn accept_gate_requires_type_then_service() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    scratch.add_default_state(&pool).await;
    let incident_type = scratch.add_type_requiring_service(&pool, "Incident").await;
    let service_id = scratch.add_service(&pool, "Payment Gateway").await;
    let st = state(&pool).await;

    let (_, issue_id) = scratch.create_item(&st, "Checkout down").await;

    let accept = || {
        patch_issue(
            State(st.clone()),
            AuthUser(scratch.user_id),
            Path((scratch.slug.clone(), scratch.project_id, issue_id)),
            Json(InboxIssuePatch {
                status: Some(1),
                ..Default::default()
            }),
        )
    };

    // 1. Type kosong, project punya tipe → 400.
    let (status, Json(body)) = accept().await.expect("accept");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("work item type"));

    // 2. Type di-set lewat PATCH intake → 200, kolom tertulis, state tetap triage.
    let (status, _) = patch_issue(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
        Json(InboxIssuePatch {
            issue: Some(InboxIssueFields {
                type_id: Some(incident_type),
                ..Default::default()
            }),
            ..Default::default()
        }),
    )
    .await
    .expect("patch type");
    assert_eq!(status, StatusCode::OK);
    let stored_type: Option<Uuid> = sqlx::query_scalar("SELECT type_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored_type, Some(incident_type));
    let state_group: String = sqlx::query_scalar(
        "SELECT s.\"group\" FROM issues i JOIN states s ON s.id = i.state_id WHERE i.id = $1",
    )
    .bind(issue_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state_group, "triage");

    // 3. Type requires_service tanpa link → 400.
    let (status, Json(body)) = accept().await.expect("accept");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("service"));

    // 4. Link service → accept lolos dan keluar dari triage.
    scratch.link_service(&pool, service_id, issue_id).await;
    let (status, _) = accept().await.expect("accept");
    assert_eq!(status, StatusCode::OK);
    let state_group: String = sqlx::query_scalar(
        "SELECT s.\"group\" FROM issues i JOIN states s ON s.id = i.state_id WHERE i.id = $1",
    )
    .bind(issue_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_ne!(state_group, "triage");

    // 5. Tipe tanpa flag → accept tanpa service.
    let (_, issue_id_2) = scratch.create_item(&st, "Printer queue").await;
    let problem_type = scratch.add_type(&pool, "Problem").await;
    patch_issue(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id_2)),
        Json(InboxIssuePatch {
            issue: Some(InboxIssueFields {
                type_id: Some(problem_type),
                ..Default::default()
            }),
            ..Default::default()
        }),
    )
    .await
    .expect("patch type");
    let problem_accept = patch_issue(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id_2)),
        Json(InboxIssuePatch {
            status: Some(1),
            ..Default::default()
        }),
    )
    .await
    .expect("accept problem");
    assert_eq!(problem_accept.0, StatusCode::OK);

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn suggestion_includes_service_and_apply_writes_type_and_link() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    let incident_type = scratch.add_type_requiring_service(&pool, "Incident").await;
    let service_id = scratch.add_service(&pool, "Payment Gateway").await;
    let st = state(&pool).await;

    let (base_url, bodies) = support::spawn_systemone_upstream(json!({
        "model": "jev-1.13.0",
        "answers": {
            "category": {"type": "choice", "choice": "Incident", "confidence": 0.9,
                         "probabilities": {"Incident": 0.9}},
            "service": {"type": "choice", "choice": "Payment Gateway", "confidence": 0.74,
                        "probabilities": {"Payment Gateway": 0.74}},
            "severity": {"type": "score", "score": 3.0, "confidence": 0.8,
                         "probabilities": {"3": 0.8}},
            "needs_human": {"type": "noul", "noul": 0.4}
        },
        "usage": {"input_tokens": 100, "output_tokens": 10}
    }))
    .await;
    set_decision_env(&base_url);

    let (row_id, issue_id) = scratch.create_item(&st, "Payments failing").await;
    ai::triage_job::classify(&pool, row_id).await.expect("classify");

    let sent = bodies.lock().unwrap().clone();
    assert!(sent[0]["questions"]["service"]["criteria"]
        .get("Payment Gateway")
        .is_some());

    let (status, Json(body)) = get_triage_suggestion(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
    )
    .await
    .expect("get suggestion");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["service"]["label"], "Payment Gateway");
    assert_eq!(body["data"]["service"]["confidence"], 0.74);

    let (status, Json(applied)) = apply_triage_suggestion(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
        Json(TriageFieldsBody {
            fields: vec!["category".to_string(), "service".to_string()],
        }),
    )
    .await
    .expect("apply");
    assert_eq!(status, StatusCode::OK);
    for field in ["category", "service"] {
        assert!(applied["data"]["applied_fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == field));
    }
    let stored_type: Option<Uuid> = sqlx::query_scalar("SELECT type_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored_type, Some(incident_type));
    let linked: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM service_issues WHERE issue_id = $1 AND service_id = $2 \
         AND deleted_at IS NULL)",
    )
    .bind(issue_id)
    .bind(service_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(linked);

    clear_decision_env();
    scratch.purge(&pool).await;
}

#[tokio::test]
async fn service_abstain_stores_sentinel_and_rejects_apply() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    scratch.add_intake(&pool).await;
    scratch.add_service(&pool, "Payment Gateway").await;
    let st = state(&pool).await;

    let (base_url, _) = support::spawn_systemone_upstream(json!({
        "model": "jev-1.13.0",
        "answers": {
            "service": {"type": "choice", "choice": "No service / unsure", "confidence": 0.6,
                        "probabilities": {}},
            "severity": {"type": "score", "score": 1.0, "confidence": 0.5,
                         "probabilities": {"1": 0.5}},
            "needs_human": {"type": "noul", "noul": 0.2}
        },
        "usage": {"input_tokens": 10, "output_tokens": 2}
    }))
    .await;
    set_decision_env(&base_url);

    let (row_id, issue_id) = scratch.create_item(&st, "General question").await;
    ai::triage_job::classify(&pool, row_id).await.expect("classify");

    let (status, Json(body)) = get_triage_suggestion(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
    )
    .await
    .expect("get suggestion");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["service"]["label"], "__none__");
    assert!(body["data"]["service"]["id"].is_null());

    let (status, _) = apply_triage_suggestion(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
        Json(TriageFieldsBody {
            fields: vec!["service".to_string()],
        }),
    )
    .await
    .expect("apply sentinel");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    clear_decision_env();
    scratch.purge(&pool).await;
}
