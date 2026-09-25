use api::middleware::auth::AuthUser;
use api::routes::workflow::{
    allowed_target_state_ids, materialize_type_states, validate_name, validate_state_group,
};
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
fn name_required_and_max_255() {
    assert!(validate_name("  ", "name").is_err());
    assert!(validate_name(&"x".repeat(256), "name").is_err());
    assert_eq!(validate_name(" Incident ", "name").unwrap(), "Incident");
}

#[test]
fn name_accepts_exactly_255_chars() {
    assert_eq!(
        validate_name(&"x".repeat(255), "name").unwrap(),
        "x".repeat(255)
    );
    assert!(validate_name(&"x".repeat(256), "name").is_err());
}

#[test]
fn name_length_counts_chars_not_bytes() {
    let name = "é".repeat(200);
    assert_eq!(name.len(), 400);
    assert_eq!(validate_name(&name, "name").unwrap(), name);
}

#[test]
fn group_must_be_known() {
    assert!(validate_state_group("backlog").is_ok());
    assert!(validate_state_group("triage").is_err());
    assert!(validate_state_group("bogus").is_err());
}

#[test]
fn group_error_names_offending_value() {
    assert_eq!(
        validate_state_group("bogus").unwrap_err(),
        "\"bogus\" is not a valid choice."
    );
    assert_eq!(
        validate_state_group("triage").unwrap_err(),
        "\"triage\" is not a valid choice."
    );
}

#[test]
fn allowed_targets_follow_transitions() {
    let mirror_new = Uuid::new_v4();
    let mirror_progress = Uuid::new_v4();
    let mirror_closed = Uuid::new_v4();
    let wf_new = Uuid::new_v4();
    let wf_progress = Uuid::new_v4();
    let wf_closed = Uuid::new_v4();
    let pairs = vec![
        (mirror_new, wf_new),
        (mirror_progress, wf_progress),
        (mirror_closed, wf_closed),
    ];
    let transitions = vec![(wf_new, wf_progress), (wf_progress, wf_closed)];

    assert_eq!(
        allowed_target_state_ids(mirror_new, &pairs, &transitions),
        vec![mirror_progress]
    );
    assert_eq!(
        allowed_target_state_ids(mirror_progress, &pairs, &transitions),
        vec![mirror_closed]
    );
    assert!(allowed_target_state_ids(mirror_closed, &pairs, &transitions).is_empty());
    assert!(allowed_target_state_ids(Uuid::new_v4(), &pairs, &transitions).is_empty());
}

#[test]
fn allowed_targets_do_not_duplicate_on_repeated_transitions() {
    let mirror_new = Uuid::new_v4();
    let mirror_progress = Uuid::new_v4();
    let wf_new = Uuid::new_v4();
    let wf_progress = Uuid::new_v4();
    let pairs = vec![(mirror_new, wf_new), (mirror_progress, wf_progress)];
    let transitions = vec![(wf_new, wf_progress), (wf_new, wf_progress)];

    assert_eq!(
        allowed_target_state_ids(mirror_new, &pairs, &transitions),
        vec![mirror_progress]
    );
}

#[test]
fn allowed_targets_exclude_unmapped_workflow_state() {
    let mirror_new = Uuid::new_v4();
    let mirror_progress = Uuid::new_v4();
    let wf_new = Uuid::new_v4();
    let wf_progress = Uuid::new_v4();
    let wf_orphan = Uuid::new_v4();
    let pairs = vec![(mirror_new, wf_new), (mirror_progress, wf_progress)];
    let transitions = vec![(wf_new, wf_progress), (wf_new, wf_orphan)];

    assert_eq!(
        allowed_target_state_ids(mirror_new, &pairs, &transitions),
        vec![mirror_progress]
    );
}

// --- DB-backed materialization test ---------------------------------------

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
    let (project_id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM projects WHERE workspace_id = $1 AND deleted_at IS NULL")
            .bind(ws_id)
            .fetch_one(&st.pool)
            .await
            .expect("project row");
    (slug, ws_id, project_id)
}

#[tokio::test]
async fn materialize_creates_and_updates_mirrors() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfm").await;

    let (type_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Incident', '', '{}', false, false, true, 0, $1, now(), now()) \
         RETURNING id",
    )
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("issue type");

    let (workflow_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO workflows (id, name, description, is_active, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Incident Workflow', '', true, $1, now(), now()) RETURNING id",
    )
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("workflow");

    sqlx::query(
        "INSERT INTO workflow_states (id, name, description, color, slug, sequence, \"group\", \
         is_default, workflow_id, created_at, updated_at) VALUES \
         (gen_random_uuid(), 'New', '', '#60646C', 'new', 15000, 'backlog', true, $1, now(), now()), \
         (gen_random_uuid(), 'In Progress', '', '#F59E0B', 'in-progress', 30000, 'started', false, $1, now(), now())",
    )
    .bind(workflow_id)
    .execute(&st.pool)
    .await
    .expect("workflow states");

    sqlx::query("UPDATE issue_types SET workflow_id = $1 WHERE id = $2")
        .bind(workflow_id)
        .bind(type_id)
        .execute(&st.pool)
        .await
        .expect("attach workflow");

    sqlx::query(
        "INSERT INTO project_issue_types (id, issue_type_id, project_id, workspace_id, level, is_default, \
         created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, 0, false, now(), now())",
    )
    .bind(type_id)
    .bind(project_id)
    .bind(ws_id)
    .execute(&st.pool)
    .await
    .expect("project type link");

    let written = materialize_type_states(&st.pool, project_id, type_id)
        .await
        .expect("materialize");
    assert_eq!(written, 2);

    let (count, defaults): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COUNT(*) FILTER (WHERE \"default\") FROM states \
         WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_one(&st.pool)
    .await
    .expect("state count");
    assert_eq!(count, 2);
    assert_eq!(defaults, 1);

    // Idempotent: panggilan kedua tidak menambah row.
    materialize_type_states(&st.pool, project_id, type_id)
        .await
        .expect("materialize again");
    let (count_again,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM states WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_one(&st.pool)
    .await
    .expect("state count 2");
    assert_eq!(count_again, 2);

    purge(&st.pool, &slug).await;
}
