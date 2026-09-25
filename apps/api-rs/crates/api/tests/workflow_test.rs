use api::middleware::auth::AuthUser;
use api::routes::workflow::{
    allowed_target_state_ids, create_state, create_workflow, delete_state, delete_workflow,
    list_states, list_workflows, materialize_type_states, patch_state, patch_workflow,
    retrieve_workflow, validate_name, validate_state_group, WorkflowBody, WorkflowStateBody,
};
use api::routes::workspace::create;
use api::state::AppState;
use axum::extract::{Path, State};
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
async fn workflow_crud_roundtrip() {
    let st = app_state().await;
    let (slug, ws_id, _project_id) = make_workspace(&st, "wfcrud").await;

    let (status, _created) = create_workflow(
        State(st.clone()),
        AuthUser(Uuid::new_v4()),
        Path(slug.clone()),
        Json(WorkflowBody {
            name: Some("Incident Workflow".into()),
            description: Some("ITIL incident".into()),
            is_active: None,
        }),
    )
    .await
    .expect("create");
    // Pemanggil bukan member workspace → 403; test kepemilikan admin ada di
    // test terpisah. Untuk roundtrip, pakai owner dari make_workspace.
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let (status, created) = create_workflow(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(WorkflowBody {
            name: Some("Incident Workflow".into()),
            description: Some("ITIL incident".into()),
            is_active: None,
        }),
    )
    .await
    .expect("create");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["description"], "ITIL incident");
    assert_eq!(created["is_active"], true);
    let workflow_id = Uuid::parse_str(created["id"].as_str().expect("id")).expect("uuid");

    // Nama duplikat → 400 dengan pesan duplikat, bukan error integritas lain.
    let (status, duplicate) = create_workflow(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(WorkflowBody {
            name: Some("Incident Workflow".into()),
            description: None,
            is_active: None,
        }),
    )
    .await
    .expect("duplicate create");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(duplicate["error"], "Workflow with this name already exists");

    // PATCH description "" + is_active false, lalu retrieve harus konsisten.
    let (status, patched) = patch_workflow(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(WorkflowBody {
            name: None,
            description: Some(String::new()),
            is_active: Some(false),
        }),
    )
    .await
    .expect("patch");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched["description"], "");
    assert_eq!(patched["is_active"], false);

    let (status, fetched) = retrieve_workflow(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
    )
    .await
    .expect("retrieve");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["description"], "");
    assert_eq!(fetched["is_active"], false);

    // Workflow kedua sengaja dibuat lebih lama → list harus DESC (baru dulu).
    let (status, _second) = create_workflow(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(WorkflowBody {
            name: Some("Second Workflow".into()),
            description: None,
            is_active: None,
        }),
    )
    .await
    .expect("create second");
    assert_eq!(status, StatusCode::CREATED);
    let (second_id,): (Uuid,) = sqlx::query_as(
        "SELECT id FROM workflows WHERE workspace_id = $1 AND name = 'Second Workflow'",
    )
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("second id");
    sqlx::query("UPDATE workflows SET created_at = now() - interval '1 day' WHERE id = $1")
        .bind(second_id)
        .execute(&st.pool)
        .await
        .expect("age second workflow");

    let (status, list) = list_workflows(State(st.clone()), AuthUser(owner), Path(slug.clone()))
        .await
        .expect("list");
    assert_eq!(status, StatusCode::OK);
    let list = list.as_array().expect("array");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["name"], "Incident Workflow");
    assert_eq!(list[1]["name"], "Second Workflow");

    // Workflow yang masih dipakai issue type hidup → 400.
    sqlx::query(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workspace_id, workflow_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Incident', '', '{}', false, false, true, 0, $1, $2, now(), now())",
    )
    .bind(ws_id)
    .bind(workflow_id)
    .execute(&st.pool)
    .await
    .expect("issue type in use");
    let (status, in_use) = delete_workflow(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
    )
    .await
    .expect("delete in use");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(in_use["error"], "Workflow is in use by a work item type");

    // Soft-delete issue type → workflow boleh dihapus.
    sqlx::query("UPDATE issue_types SET deleted_at = now() WHERE workflow_id = $1")
        .bind(workflow_id)
        .execute(&st.pool)
        .await
        .expect("detach issue type");
    let (status, _) = delete_workflow(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
    )
    .await
    .expect("delete");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, list) = list_workflows(State(st.clone()), AuthUser(owner), Path(slug.clone()))
        .await
        .expect("list after delete");
    let list = list.as_array().expect("array");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["name"], "Second Workflow");

    purge(&st.pool, &slug).await;
    let (workflows_left, types_left): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM workflows WHERE workspace_id = $1), \
                (SELECT COUNT(*) FROM issue_types WHERE workspace_id = $1)",
    )
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("scratch rows");
    assert_eq!(workflows_left, 0);
    assert_eq!(types_left, 0);
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

#[tokio::test]
async fn materialize_switches_workflow_and_clears_old_mirrors() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfs").await;

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

    let (wf_a,): (Uuid,) = sqlx::query_as(
        "INSERT INTO workflows (id, name, description, is_active, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Workflow A', '', true, $1, now(), now()) RETURNING id",
    )
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("workflow A");

    let (wf_b,): (Uuid,) = sqlx::query_as(
        "INSERT INTO workflows (id, name, description, is_active, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Workflow B', '', true, $1, now(), now()) RETURNING id",
    )
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("workflow B");

    // Nama state sengaja sama di kedua workflow untuk memicu tabrakan
    // partial-unique (project_id, type_id, name) saat switch.
    sqlx::query(
        "INSERT INTO workflow_states (id, name, description, color, slug, sequence, \"group\", \
         is_default, workflow_id, created_at, updated_at) VALUES \
         (gen_random_uuid(), 'New', '', '#60646C', 'new', 15000, 'backlog', true, $1, now(), now()), \
         (gen_random_uuid(), 'Closed', '', '#22C55E', 'closed', 30000, 'completed', false, $1, now(), now())",
    )
    .bind(wf_a)
    .execute(&st.pool)
    .await
    .expect("workflow A states");

    sqlx::query(
        "INSERT INTO workflow_states (id, name, description, color, slug, sequence, \"group\", \
         is_default, workflow_id, created_at, updated_at) VALUES \
         (gen_random_uuid(), 'New', '', '#60646C', 'new', 15000, 'backlog', false, $1, now(), now()), \
         (gen_random_uuid(), 'Closed', '', '#22C55E', 'closed', 30000, 'completed', true, $1, now(), now())",
    )
    .bind(wf_b)
    .execute(&st.pool)
    .await
    .expect("workflow B states");

    let (a_new,): (Uuid,) =
        sqlx::query_as("SELECT id FROM workflow_states WHERE workflow_id = $1 AND name = 'New'")
            .bind(wf_a)
            .fetch_one(&st.pool)
            .await
            .expect("A New");
    let (a_closed,): (Uuid,) =
        sqlx::query_as("SELECT id FROM workflow_states WHERE workflow_id = $1 AND name = 'Closed'")
            .bind(wf_a)
            .fetch_one(&st.pool)
            .await
            .expect("A Closed");
    let (b_new,): (Uuid,) =
        sqlx::query_as("SELECT id FROM workflow_states WHERE workflow_id = $1 AND name = 'New'")
            .bind(wf_b)
            .fetch_one(&st.pool)
            .await
            .expect("B New");
    let (b_closed,): (Uuid,) =
        sqlx::query_as("SELECT id FROM workflow_states WHERE workflow_id = $1 AND name = 'Closed'")
            .bind(wf_b)
            .fetch_one(&st.pool)
            .await
            .expect("B Closed");

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

    sqlx::query("UPDATE issue_types SET workflow_id = $1 WHERE id = $2")
        .bind(wf_a)
        .bind(type_id)
        .execute(&st.pool)
        .await
        .expect("attach workflow A");
    let written_a = materialize_type_states(&st.pool, project_id, type_id)
        .await
        .expect("materialize A");
    assert_eq!(written_a, 2);

    // Switch ke workflow B: mirror lama harus di-soft-delete lebih dulu,
    // jika tidak INSERT 'New'/'Closed' menabrak partial-unique name (23505).
    sqlx::query("UPDATE issue_types SET workflow_id = $1 WHERE id = $2")
        .bind(wf_b)
        .bind(type_id)
        .execute(&st.pool)
        .await
        .expect("switch to workflow B");
    let written_b = materialize_type_states(&st.pool, project_id, type_id)
        .await
        .expect("materialize B after switch");
    assert_eq!(written_b, 2);

    let (old_live, old_total): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE deleted_at IS NULL), COUNT(*) FROM states \
         WHERE project_id = $1 AND type_id = $2 AND workflow_state_id IN ($3, $4)",
    )
    .bind(project_id)
    .bind(type_id)
    .bind(a_new)
    .bind(a_closed)
    .fetch_one(&st.pool)
    .await
    .expect("old mirror counts");
    assert_eq!(old_live, 0, "mirror workflow lama harus soft-deleted");
    assert_eq!(old_total, 2, "baris lama tetap ada sebagai soft-deleted");

    let (new_live, defaults_live): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COUNT(*) FILTER (WHERE \"default\") FROM states \
         WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL \
           AND workflow_state_id IN ($3, $4)",
    )
    .bind(project_id)
    .bind(type_id)
    .bind(b_new)
    .bind(b_closed)
    .fetch_one(&st.pool)
    .await
    .expect("new mirror counts");
    assert_eq!(new_live, 2);
    assert_eq!(defaults_live, 1);

    let (live_total,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM states WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_one(&st.pool)
    .await
    .expect("live total");
    assert_eq!(live_total, 2);

    // Idempotent: panggilan kedua hanya memproses 2 mirror B, tanpa insert.
    let written_b_again = materialize_type_states(&st.pool, project_id, type_id)
        .await
        .expect("materialize B again");
    assert_eq!(written_b_again, 2);
    let (old_live_again, new_live_again, defaults_again): (i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE workflow_state_id IN ($3, $4) AND deleted_at IS NULL), \
                COUNT(*) FILTER (WHERE workflow_state_id IN ($5, $6) AND deleted_at IS NULL), \
                COUNT(*) FILTER (WHERE deleted_at IS NULL AND \"default\") \
         FROM states WHERE project_id = $1 AND type_id = $2",
    )
    .bind(project_id)
    .bind(type_id)
    .bind(a_new)
    .bind(a_closed)
    .bind(b_new)
    .bind(b_closed)
    .fetch_one(&st.pool)
    .await
    .expect("counts after idempotent call");
    assert_eq!(old_live_again, 0);
    assert_eq!(new_live_again, 2);
    assert_eq!(defaults_again, 1);

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn state_create_syncs_mirror_and_default() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfstate").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let (_, created) = create_workflow(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(WorkflowBody {
            name: Some("Incident Workflow".into()),
            description: None,
            is_active: None,
        }),
    )
    .await
    .expect("workflow");
    let workflow_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    let (type_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workflow_id, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Incident', '', '{}', false, false, true, 0, $1, $2, now(), now()) \
         RETURNING id",
    )
    .bind(workflow_id)
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("type");
    sqlx::query(
        "INSERT INTO project_issue_types (id, issue_type_id, project_id, workspace_id, level, is_default, \
         created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, 0, false, now(), now())",
    )
    .bind(type_id)
    .bind(project_id)
    .bind(ws_id)
    .execute(&st.pool)
    .await
    .expect("link");

    let (status, created_state) = create_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(WorkflowStateBody {
            name: Some("New".into()),
            description: None,
            color: None,
            group: None,
            sequence: None,
            is_default: None,
        }),
    )
    .await
    .expect("state");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created_state["name"], "New");
    assert_eq!(created_state["slug"], "new");
    assert_eq!(created_state["is_default"], true);
    let workflow_state_id =
        Uuid::parse_str(created_state["id"].as_str().expect("state id")).expect("state uuid");

    let (mirror_state_id, mirror_workflow_state_id, name, is_default, type_match): (
        Uuid,
        Uuid,
        String,
        bool,
        Uuid,
    ) = sqlx::query_as(
        "SELECT s.id, s.workflow_state_id, s.name, s.\"default\", s.type_id FROM states s \
         WHERE s.project_id = $1 AND s.workflow_state_id IS NOT NULL AND s.deleted_at IS NULL",
    )
    .bind(project_id)
    .fetch_one(&st.pool)
    .await
    .expect("mirror");
    assert_eq!(mirror_workflow_state_id, workflow_state_id);
    assert_eq!(name, "New");
    assert!(is_default, "state pertama otomatis jadi default");
    assert_eq!(type_match, type_id);

    // Hapus state default ditolak (workflow wajib selalu punya satu default).
    let (status, body) = delete_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id, workflow_state_id)),
    )
    .await
    .expect("delete default");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"],
        "Cannot delete the default workflow state; set another state as default first"
    );
    let (still_there,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM workflow_states WHERE id = $1 AND deleted_at IS NULL")
            .bind(workflow_state_id)
            .fetch_one(&st.pool)
            .await
            .expect("state alive");
    assert_eq!(still_there, 1, "state default tidak boleh ter-soft-delete");

    // Rename harus recompute slug dan men-sync mirror project.
    let (status, renamed) = patch_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id, workflow_state_id)),
        Json(WorkflowStateBody {
            name: Some("New State".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("rename");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(renamed["slug"], "new-state");
    let (mirror_name,): (String,) = sqlx::query_as("SELECT name FROM states WHERE id = $1")
        .bind(mirror_state_id)
        .fetch_one(&st.pool)
        .await
        .expect("mirror renamed");
    assert_eq!(mirror_name, "New State");

    // Nama duplikat di workflow yang sama → 400 pesan duplikat, bukan error lain.
    let (status, duplicate) = create_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(WorkflowStateBody {
            name: Some("New State".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("duplicate state");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        duplicate["error"],
        "Workflow state with this name already exists"
    );

    // State non-default boleh dihapus: row workflow_state di-soft-delete dan
    // mirror project ikut hilang lewat sync.
    let (_, done) = create_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(WorkflowStateBody {
            name: Some("Done".into()),
            group: Some("completed".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("second state");
    assert_eq!(done["is_default"], false);
    let done_state_id = Uuid::parse_str(done["id"].as_str().expect("done id")).expect("uuid");
    let (status, _) = delete_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id, done_state_id)),
    )
    .await
    .expect("delete non-default");
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (state_alive, mirror_alive): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM workflow_states WHERE id = $1 AND deleted_at IS NULL), \
                (SELECT COUNT(*) FROM states WHERE workflow_state_id = $1 AND deleted_at IS NULL)",
    )
    .bind(done_state_id)
    .fetch_one(&st.pool)
    .await
    .expect("deleted state gone");
    assert_eq!(state_alive, 0);
    assert_eq!(mirror_alive, 0);

    let (_, list) = list_states(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
    )
    .await
    .expect("list");
    assert_eq!(list.as_array().unwrap().len(), 1);

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn state_default_promote_and_guards() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfpromo").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let (_, created) = create_workflow(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(WorkflowBody {
            name: Some("Incident Workflow".into()),
            description: None,
            is_active: None,
        }),
    )
    .await
    .expect("workflow");
    let workflow_id = Uuid::parse_str(created["id"].as_str().expect("workflow id")).expect("uuid");

    let (type_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workflow_id, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Incident', '', '{}', false, false, true, 0, $1, $2, now(), now()) \
         RETURNING id",
    )
    .bind(workflow_id)
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("type");
    sqlx::query(
        "INSERT INTO project_issue_types (id, issue_type_id, project_id, workspace_id, level, is_default, \
         created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, 0, false, now(), now())",
    )
    .bind(type_id)
    .bind(project_id)
    .bind(ws_id)
    .execute(&st.pool)
    .await
    .expect("link");

    let (status, first) = create_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(WorkflowStateBody {
            name: Some("New".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("first state");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(first["is_default"], true);
    let first_id = Uuid::parse_str(first["id"].as_str().expect("first id")).expect("uuid");

    let (status, second) = create_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(WorkflowStateBody {
            name: Some("In Progress".into()),
            group: Some("started".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("second state");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(second["is_default"], false);
    let second_id = Uuid::parse_str(second["id"].as_str().expect("second id")).expect("uuid");

    // Promote state kedua: default lama turun, mirror project ikut pindah.
    let (status, promoted) = patch_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id, second_id)),
        Json(WorkflowStateBody {
            is_default: Some(true),
            ..Default::default()
        }),
    )
    .await
    .expect("promote");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(promoted["is_default"], true);

    let (default_count, default_name): (i64, String) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE is_default), \
                COALESCE(MAX(name) FILTER (WHERE is_default), '') \
         FROM workflow_states WHERE workflow_id = $1 AND deleted_at IS NULL",
    )
    .bind(workflow_id)
    .fetch_one(&st.pool)
    .await
    .expect("default count");
    assert_eq!(default_count, 1, "hanya satu default di workflow_states");
    assert_eq!(default_name, "In Progress");

    let (first_is_default,): (bool,) =
        sqlx::query_as("SELECT is_default FROM workflow_states WHERE id = $1")
            .bind(first_id)
            .fetch_one(&st.pool)
            .await
            .expect("first row");
    assert!(!first_is_default, "default lama harus turun");

    let (mirror_default,): (String,) = sqlx::query_as(
        "SELECT ws.name FROM states s JOIN workflow_states ws ON ws.id = s.workflow_state_id \
         WHERE s.project_id = $1 AND s.deleted_at IS NULL AND s.\"default\"",
    )
    .bind(project_id)
    .fetch_one(&st.pool)
    .await
    .expect("mirror default");
    assert_eq!(mirror_default, "In Progress");

    // Menurunkan default yang aktif ditolak.
    let (status, body) = patch_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id, second_id)),
        Json(WorkflowStateBody {
            is_default: Some(false),
            ..Default::default()
        }),
    )
    .await
    .expect("demote current default");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "A workflow must have a default state");

    // State yang dipakai issue hidup tidak boleh dihapus.
    let (mirror_first,): (Uuid,) = sqlx::query_as(
        "SELECT id FROM states WHERE project_id = $1 AND workflow_state_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(first_id)
    .fetch_one(&st.pool)
    .await
    .expect("first mirror");
    sqlx::query(
        "INSERT INTO issues (id, name, description_html, description_json, priority, is_draft, \
         sort_order, sequence_id, state_id, project_id, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Blocked work', '<p></p>', '{}', 'none', true, \
         65535, (SELECT COALESCE(MAX(sequence_id), 0) + 1000 FROM issues WHERE project_id = $2), \
         $1, $2, $3, now(), now())",
    )
    .bind(mirror_first)
    .bind(project_id)
    .bind(ws_id)
    .execute(&st.pool)
    .await
    .expect("issue in use");
    let (status, body) = delete_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id, first_id)),
    )
    .await
    .expect("delete in use");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Workflow state is in use by work items");
    sqlx::query("UPDATE issues SET deleted_at = now() WHERE state_id = $1")
        .bind(mirror_first)
        .execute(&st.pool)
        .await
        .expect("close issue");

    // Slug di-trim dari karakter non-alphanumerik di ujung.
    let (status, trimmed) = create_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(WorkflowStateBody {
            name: Some("Review!".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("trimmed slug");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(trimmed["slug"], "review");

    // Drift nol-default (mis. hasil import) diheal: state baru jadi default.
    sqlx::query("UPDATE workflow_states SET is_default = false WHERE workflow_id = $1")
        .bind(workflow_id)
        .execute(&st.pool)
        .await
        .expect("drop all defaults");
    let (status, healed) = create_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(WorkflowStateBody {
            name: Some("Closed".into()),
            group: Some("completed".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("healed state");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(healed["is_default"], true);

    purge(&st.pool, &slug).await;
}
