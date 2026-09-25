use api::middleware::auth::AuthUser;
use api::routes::issue_common::resolve_issue_state;
use api::routes::workflow::{evaluate_transition, TransitionContext};
use api::routes::workspace::create;
use api::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use common::config::AppConfig;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

#[test]
fn same_state_is_noop() {
    let ctx = TransitionContext {
        pairs: vec![],
        transitions: vec![],
        default_state_id: None,
    };
    let state = Uuid::new_v4();
    assert!(evaluate_transition(state, state, &ctx).is_ok());
}

#[test]
fn transition_must_exist() {
    let mirror_new = Uuid::new_v4();
    let mirror_progress = Uuid::new_v4();
    let mirror_closed = Uuid::new_v4();
    let wf_new = Uuid::new_v4();
    let wf_progress = Uuid::new_v4();
    let wf_closed = Uuid::new_v4();
    let ctx = TransitionContext {
        pairs: vec![
            (mirror_new, wf_new),
            (mirror_progress, wf_progress),
            (mirror_closed, wf_closed),
        ],
        transitions: vec![(wf_new, wf_progress), (wf_progress, wf_closed)],
        default_state_id: Some(mirror_new),
    };
    assert!(evaluate_transition(mirror_new, mirror_progress, &ctx).is_ok());
    assert!(evaluate_transition(mirror_progress, mirror_closed, &ctx).is_ok());
    assert_eq!(
        evaluate_transition(mirror_new, mirror_closed, &ctx),
        Err(vec![mirror_progress])
    );
    assert_eq!(
        evaluate_transition(mirror_closed, mirror_new, &ctx),
        Err(vec![])
    );
    // Target di luar workflow type → tidak ada yang diizinkan.
    assert_eq!(
        evaluate_transition(mirror_new, Uuid::new_v4(), &ctx),
        Err(vec![])
    );
}

#[test]
fn legacy_current_state_only_moves_to_default() {
    let mirror_new = Uuid::new_v4();
    let legacy_state = Uuid::new_v4();
    let wf_new = Uuid::new_v4();
    let ctx = TransitionContext {
        pairs: vec![(mirror_new, wf_new)],
        transitions: vec![],
        default_state_id: Some(mirror_new),
    };
    assert!(evaluate_transition(legacy_state, mirror_new, &ctx).is_ok());
    assert_eq!(
        evaluate_transition(legacy_state, Uuid::new_v4(), &ctx),
        Err(vec![mirror_new])
    );
}

#[test]
fn legacy_without_default_denies_everything() {
    let mirror_new = Uuid::new_v4();
    let legacy_state = Uuid::new_v4();
    let ctx = TransitionContext {
        pairs: vec![(mirror_new, Uuid::new_v4())],
        transitions: vec![],
        default_state_id: None,
    };
    assert_eq!(
        evaluate_transition(legacy_state, mirror_new, &ctx),
        Err(vec![])
    );
}

#[test]
fn legacy_target_inside_workflow_still_must_be_default() {
    let mirror_new = Uuid::new_v4();
    let mirror_progress = Uuid::new_v4();
    let legacy_state = Uuid::new_v4();
    let wf_new = Uuid::new_v4();
    let wf_progress = Uuid::new_v4();
    let ctx = TransitionContext {
        pairs: vec![(mirror_new, wf_new), (mirror_progress, wf_progress)],
        transitions: vec![(wf_new, wf_progress)],
        default_state_id: Some(mirror_new),
    };
    assert_eq!(
        evaluate_transition(legacy_state, mirror_progress, &ctx),
        Err(vec![mirror_new])
    );
}

// --- DB-backed default-state resolution test -------------------------------

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

async fn app_state() -> AppState {
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

/// Hapus workspace uji beserta seluruh baris turunannya. Urutan penting:
/// FK Django `DEFERRABLE INITIALLY DEFERRED` tetap dicek saat commit tiap
/// statement (autocommit), jadi child harus dihapus sebelum parent.
async fn purge(pool: &PgPool, slug: &str) {
    let ws: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM workspaces WHERE slug = $1")
        .bind(slug)
        .fetch_optional(pool)
        .await
        .expect("purge lookup");
    let Some((ws_id,)) = ws else { return };
    let members: Vec<(Uuid,)> =
        sqlx::query_as("SELECT member_id FROM workspace_members WHERE workspace_id = $1")
            .bind(ws_id)
            .fetch_all(pool)
            .await
            .expect("purge members");
    for stmt in [
        "DELETE FROM workflow_transitions WHERE workflow_id IN \
         (SELECT id FROM workflows WHERE workspace_id = $1)",
        "DELETE FROM module_issues WHERE workspace_id = $1",
        "DELETE FROM cycle_issues WHERE workspace_id = $1",
        "DELETE FROM issue_labels WHERE workspace_id = $1",
        "DELETE FROM issue_activities WHERE workspace_id = $1",
        "DELETE FROM issue_sequences WHERE workspace_id = $1",
        "DELETE FROM issues WHERE workspace_id = $1",
        "DELETE FROM issue_views WHERE workspace_id = $1",
        "DELETE FROM states WHERE workspace_id = $1",
        "DELETE FROM project_issue_types WHERE workspace_id = $1",
        "DELETE FROM issue_types WHERE workspace_id = $1",
        "DELETE FROM workflow_states WHERE workflow_id IN \
         (SELECT id FROM workflows WHERE workspace_id = $1)",
        "DELETE FROM workflows WHERE workspace_id = $1",
        "DELETE FROM project_pages WHERE workspace_id = $1",
        "DELETE FROM pages WHERE workspace_id = $1",
        "DELETE FROM modules WHERE workspace_id = $1",
        "DELETE FROM cycles WHERE workspace_id = $1",
        "DELETE FROM labels WHERE workspace_id = $1",
        "DELETE FROM project_user_properties WHERE workspace_id = $1",
        "DELETE FROM project_members WHERE workspace_id = $1",
        "DELETE FROM projects WHERE workspace_id = $1",
        "DELETE FROM workspace_members WHERE workspace_id = $1",
        "DELETE FROM workspaces WHERE id = $1",
    ] {
        sqlx::query(stmt)
            .bind(ws_id)
            .execute(pool)
            .await
            .expect("purge");
    }
    for (member_id,) in members {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(member_id)
            .execute(pool)
            .await
            .expect("purge user");
    }
}

async fn make_workspace(st: &AppState, prefix: &str) -> (String, Uuid, Uuid) {
    let slug = format!("{prefix}-{}", Uuid::new_v4().simple());
    let owner = Uuid::new_v4();
    insert_user(&st.pool, owner, &slug).await;
    let (status, _) = create(
        State(st.clone()),
        AuthUser(owner),
        Json(json!({"name": "Acme IT", "slug": slug})),
    )
    .await
    .expect("workspace create");
    assert_eq!(status, StatusCode::CREATED);
    let (ws_id,): (Uuid,) = sqlx::query_as("SELECT id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("workspace row");
    for stmt in [
        "DELETE FROM workflow_transitions WHERE workflow_id IN (SELECT id FROM workflows WHERE workspace_id = $1)",
        "DELETE FROM workflow_states WHERE workflow_id IN (SELECT id FROM workflows WHERE workspace_id = $1)",
        "DELETE FROM issue_types WHERE workspace_id = $1",
        "DELETE FROM workflows WHERE workspace_id = $1",
    ] {
        sqlx::query(stmt).bind(ws_id).execute(&st.pool).await.expect("seed cleanup");
    }
    let (project_id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM projects WHERE workspace_id = $1 AND deleted_at IS NULL")
            .bind(ws_id)
            .fetch_one(&st.pool)
            .await
            .expect("project row");
    (slug, ws_id, project_id)
}

#[tokio::test]
async fn typed_default_beats_legacy_default() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfdefault").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let (type_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Incident', '', '{}', false, false, true, 0, $1, now(), now()) \
         RETURNING id",
    )
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("type");
    let (typed_state,): (Uuid,) = sqlx::query_as(
        "INSERT INTO states (id, name, description, color, slug, sequence, \"group\", is_triage, \
         \"default\", project_id, workspace_id, type_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'New', '', '#60646C', 'new', 15000, 'backlog', false, true, \
         $1, $2, $3, now(), now()) RETURNING id",
    )
    .bind(project_id)
    .bind(ws_id)
    .bind(type_id)
    .fetch_one(&st.pool)
    .await
    .expect("typed state");

    let resolved = resolve_issue_state(&st.pool, project_id, Some(type_id), None)
        .await
        .expect("resolve");
    assert_eq!(resolved, Some(typed_state));

    // Default legacy tetap ada, tetapi kalah dari default milik type.
    let (legacy_default,): (Uuid,) = sqlx::query_as(
        "SELECT id FROM states WHERE project_id = $1 AND deleted_at IS NULL \
         AND \"group\" != 'triage' AND is_triage = false AND \"default\" = true \
         ORDER BY sequence ASC, created_at ASC LIMIT 1",
    )
    .bind(project_id)
    .fetch_one(&st.pool)
    .await
    .expect("legacy default");
    assert_ne!(legacy_default, typed_state);

    // Explicit menang atas default typed.
    let explicit = Uuid::new_v4();
    let resolved = resolve_issue_state(&st.pool, project_id, Some(type_id), Some(explicit))
        .await
        .expect("resolve explicit");
    assert_eq!(resolved, Some(explicit));

    // Tanpa type, resolver jatuh ke default legacy.
    let resolved = resolve_issue_state(&st.pool, project_id, None, None)
        .await
        .expect("resolve untyped");
    assert_eq!(resolved, Some(legacy_default));

    let _ = owner;
    purge(&st.pool, &slug).await;
}
