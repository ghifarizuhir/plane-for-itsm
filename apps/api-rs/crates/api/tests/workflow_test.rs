use api::middleware::auth::AuthUser;
use api::routes::v1::work_item_type::{
    create_workspace, delete_project, delete_workspace, import_to_project, update_workspace,
    V1CreateWorkItemType, V1UpdateWorkItemType,
};
use api::routes::workflow::{
    allowed_target_state_ids, create_state, create_transition, create_workflow, delete_state,
    delete_transition, delete_workflow, derived_workflow_name, ensure_workflow_for_type, list_states,
    list_transitions, list_workflows, materialize_type_states, patch_state, patch_workflow,
    retrieve_workflow, soft_delete_workflow_cascade, sync_type_workflow, unlink_type, validate_name,
    validate_state_group, workflow_map, TransitionBody, WorkflowBody, WorkflowStateBody,
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
fn derived_workflow_name_appends_suffix() {
    assert_eq!(derived_workflow_name("Incident"), "Incident Workflow");
    assert_eq!(derived_workflow_name("  Incident  "), "Incident Workflow");
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

#[tokio::test]
async fn transition_crud_validates_states() {
    let st = app_state().await;
    let (slug, _ws_id, _project_id) = make_workspace(&st, "wftr").await;
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

    let (_, new) = create_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(WorkflowStateBody {
            name: Some("New".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("state new");
    let (_, progress) = create_state(
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
    .expect("state progress");
    let new_id = Uuid::parse_str(new["id"].as_str().unwrap()).unwrap();
    let progress_id = Uuid::parse_str(progress["id"].as_str().unwrap()).unwrap();

    let (status, created_transition) = create_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(TransitionBody {
            from_state_id: Some(new_id),
            to_state_id: Some(progress_id),
        }),
    )
    .await
    .expect("transition");
    assert_eq!(status, StatusCode::CREATED);
    let transition_id = Uuid::parse_str(created_transition["id"].as_str().unwrap()).unwrap();

    // Duplikat → 400.
    let (status, body) = create_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(TransitionBody {
            from_state_id: Some(new_id),
            to_state_id: Some(progress_id),
        }),
    )
    .await
    .expect("dup");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "This transition already exists");

    // Self transition → 400.
    let (status, _) = create_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(TransitionBody {
            from_state_id: Some(new_id),
            to_state_id: Some(new_id),
        }),
    )
    .await
    .expect("self");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Salah satu id hilang → 400 dengan pesan required.
    let (status, body) = create_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(TransitionBody {
            from_state_id: Some(new_id),
            to_state_id: None,
        }),
    )
    .await
    .expect("missing to_state_id");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "from_state_id and to_state_id are required");

    let (_, list) = list_transitions(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
    )
    .await
    .expect("list");
    assert_eq!(list.as_array().unwrap().len(), 1);

    // State milik workflow lain di workspace yang sama → 400.
    let (_, other) = create_workflow(
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
    .expect("second workflow");
    let other_workflow_id = Uuid::parse_str(other["id"].as_str().unwrap()).unwrap();
    let (_, other_state) = create_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), other_workflow_id)),
        Json(WorkflowStateBody {
            name: Some("Other".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("other state");
    let other_state_id = Uuid::parse_str(other_state["id"].as_str().unwrap()).unwrap();
    let (status, body) = create_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(TransitionBody {
            from_state_id: Some(new_id),
            to_state_id: Some(other_state_id),
        }),
    )
    .await
    .expect("cross workflow");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "States must belong to this workflow");

    // `from_state_id` dari workflow lain juga ditolak (sisi kiri `||`).
    let (status, body) = create_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(TransitionBody {
            from_state_id: Some(other_state_id),
            to_state_id: Some(new_id),
        }),
    )
    .await
    .expect("cross workflow from");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "States must belong to this workflow");

    let (status, _) = delete_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id, transition_id)),
    )
    .await
    .expect("delete transition");
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = delete_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id, transition_id)),
    )
    .await
    .expect("delete transition twice");
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, list) = list_transitions(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
    )
    .await
    .expect("list after delete");
    assert!(list.as_array().unwrap().is_empty());

    // Partial unique hanya berlaku untuk baris hidup: pasangan yang sama
    // boleh dibuat ulang setelah soft-delete.
    let (status, _) = create_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(TransitionBody {
            from_state_id: Some(new_id),
            to_state_id: Some(progress_id),
        }),
    )
    .await
    .expect("recreate after delete");
    assert_eq!(status, StatusCode::CREATED);
    let (_, list) = list_transitions(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
    )
    .await
    .expect("list after recreate");
    assert_eq!(list.as_array().unwrap().len(), 1);

    // Workflow ter-soft-delete → list 404, bukan list kosong.
    let (_, doomed) = create_workflow(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(WorkflowBody {
            name: Some("Doomed Workflow".into()),
            description: None,
            is_active: None,
        }),
    )
    .await
    .expect("doomed workflow");
    let doomed_id = Uuid::parse_str(doomed["id"].as_str().unwrap()).unwrap();
    sqlx::query("UPDATE workflows SET deleted_at = now() WHERE id = $1")
        .bind(doomed_id)
        .execute(&st.pool)
        .await
        .expect("soft-delete workflow");
    let (status, body) = list_transitions(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), doomed_id)),
    )
    .await
    .expect("list deleted workflow");
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "The required object does not exist.");

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn import_materializes_and_unlink_guards() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wftype").await;
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
    let _ = create_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(WorkflowStateBody {
            name: Some("New".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("state");
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

    let (status, _) = import_to_project(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
        Json(json!({"work_item_types": [type_id]})),
    )
    .await
    .expect("import");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (mirrors,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM states WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_one(&st.pool)
    .await
    .expect("mirrors");
    assert_eq!(mirrors, 1);

    let (status, map) = workflow_map(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
    )
    .await
    .expect("map");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(map["types"].as_array().unwrap().len(), 1);
    assert_eq!(map["types"][0]["type_id"], type_id.to_string());
    assert_eq!(map["types"][0]["states"].as_array().unwrap().len(), 1);

    // Body `workflow` eksplisit ditolak (workflow dikelola lewat type).
    let explicit_workflow: V1UpdateWorkItemType =
        serde_json::from_value(json!({"workflow": null})).unwrap();
    let (status, body) = update_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), type_id)),
        Json(explicit_workflow),
    )
    .await
    .expect("patch workflow");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Workflows are managed through work item types");

    // Type kedua dengan workflow sendiri boleh di-import ke project yang sama.
    let (status, second_type) = create_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(V1CreateWorkItemType {
            name: Some("Problem".into()),
            project_ids: vec![project_id],
            ..Default::default()
        }),
    )
    .await
    .expect("second type");
    assert_eq!(status, StatusCode::CREATED);
    let second_type_id = Uuid::parse_str(second_type["id"].as_str().unwrap()).unwrap();
    let (links,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM project_issue_types WHERE project_id = $1 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .fetch_one(&st.pool)
    .await
    .expect("links after second import");
    assert_eq!(links, 2);
    // Bersihkan lagi supaya assertion map berikutnya (project kosong) tetap valid.
    let (status, _) = delete_project(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id, second_type_id)),
    )
    .await
    .expect("detach second");
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Hapus type ditolak selama masih ada link hidup ke project (spec), baik
    // lewat jalur workspace maupun project.
    let (status, body) = delete_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), type_id)),
    )
    .await
    .expect("delete linked type");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Type is enabled in projects");
    let (still_live,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM issue_types WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(type_id)
    .fetch_one(&st.pool)
    .await
    .expect("type row");
    assert!(still_live, "type tidak boleh ter-soft-delete");

    // Project-scope DELETE = detach link + mirror, 204 (idempotent).
    let (status, _) = delete_project(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id, type_id)),
    )
    .await
    .expect("detach via delete");
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (links_still_live, mirrors_still_live): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM project_issue_types \
                 WHERE project_id = $1 AND issue_type_id = $2 AND deleted_at IS NULL), \
                (SELECT COUNT(*) FROM states \
                 WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_one(&st.pool)
    .await
    .expect("link after detach");
    assert_eq!(
        links_still_live, 0,
        "project-scope delete harus detach link"
    );
    assert_eq!(
        mirrors_still_live, 0,
        "project-scope delete harus soft-delete mirror"
    );
    let (status, _) = delete_project(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id, type_id)),
    )
    .await
    .expect("detach via delete again");
    assert_eq!(status, StatusCode::NO_CONTENT, "delete kedua idempotent");

    // Un-enable saat belum ada issue → 204.
    let (status, _) = unlink_type(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id, type_id)),
    )
    .await
    .expect("unlink");
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Un-enable men-soft-delete mirror type dan mengosongkan workflow-map.
    let (live_mirrors, total_mirrors): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE deleted_at IS NULL), COUNT(*) FROM states \
         WHERE project_id = $1 AND type_id = $2",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_one(&st.pool)
    .await
    .expect("mirrors after unlink");
    assert_eq!(live_mirrors, 0, "unlink harus men-soft-delete mirror");
    assert_eq!(total_mirrors, 1);

    let (status, map) = workflow_map(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
    )
    .await
    .expect("map after unlink");
    assert_eq!(status, StatusCode::OK);
    assert!(map["types"].as_array().unwrap().is_empty());

    // Create via API tanpa `workflow`: workflow derived + materialize.
    let (status, created_type) = create_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(V1CreateWorkItemType {
            name: Some("Problem Type".into()),
            project_ids: vec![project_id],
            ..Default::default()
        }),
    )
    .await
    .expect("create with project_ids");
    assert_eq!(status, StatusCode::CREATED);
    let created_type_id = Uuid::parse_str(created_type["id"].as_str().unwrap()).unwrap();
    let created_workflow_id = Uuid::parse_str(created_type["workflow"].as_str().unwrap()).unwrap();
    let (workflow_name,): (String,) = sqlx::query_as("SELECT name FROM workflows WHERE id = $1")
        .bind(created_workflow_id)
        .fetch_one(&st.pool)
        .await
        .expect("derived workflow");
    assert_eq!(workflow_name, "Problem Type Workflow");
    let (created_mirrors,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM states WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(created_type_id)
    .fetch_one(&st.pool)
    .await
    .expect("created type mirrors");
    assert_eq!(
        created_mirrors, 1,
        "create dengan project_ids harus materialize"
    );

    // Update project_ids (workflow diwarisi) tetap idempotent.
    let (status, patched_type) = update_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), created_type_id)),
        Json(V1UpdateWorkItemType {
            project_ids: Some(vec![project_id]),
            ..Default::default()
        }),
    )
    .await
    .expect("update with project_ids");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched_type["workflow"], created_workflow_id.to_string());

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn type_workflow_lifecycle_syncs_name_active_and_delete() {
    let st = app_state().await;
    let (slug, _ws_id, project_id) = make_workspace(&st, "wflife").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    // Create type via API → workflow derived + default state.
    let (status, created) = create_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(V1CreateWorkItemType {
            name: Some("Incident".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("create type");
    assert_eq!(status, StatusCode::CREATED);
    let type_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    let workflow_id = Uuid::parse_str(created["workflow"].as_str().unwrap()).unwrap();
    let (name,): (String,) = sqlx::query_as("SELECT name FROM workflows WHERE id = $1")
        .bind(workflow_id)
        .fetch_one(&st.pool)
        .await
        .expect("workflow name");
    assert_eq!(name, "Incident Workflow");

    // Tambah state + transisi lalu import ke project; cek detail workflow-map.
    let (_, progress) = create_state(
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
    .expect("state");
    let progress_id = Uuid::parse_str(progress["id"].as_str().unwrap()).unwrap();
    let (_, states) = list_states(State(st.clone()), AuthUser(owner), Path((slug.clone(), workflow_id)))
        .await
        .expect("states");
    let default_id = Uuid::parse_str(states[0]["id"].as_str().unwrap()).unwrap();
    let _ = create_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(TransitionBody {
            from_state_id: Some(default_id),
            to_state_id: Some(progress_id),
        }),
    )
    .await
    .expect("transition");
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = import_to_project(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
        Json(json!({"work_item_types": [type_id]})),
    )
    .await
    .expect("import");
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, map) = workflow_map(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
    )
    .await
    .expect("map");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(map["types"][0]["type_name"], "Incident");
    assert_eq!(map["types"][0]["states"].as_array().unwrap().len(), 2);
    assert_eq!(map["types"][0]["transitions"].as_array().unwrap().len(), 1);

    // Rename type → workflow ikut rename.
    let (status, _) = update_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), type_id)),
        Json(V1UpdateWorkItemType {
            name: Some("Major Incident".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("rename");
    assert_eq!(status, StatusCode::OK);
    let (renamed,): (String,) = sqlx::query_as("SELECT name FROM workflows WHERE id = $1")
        .bind(workflow_id)
        .fetch_one(&st.pool)
        .await
        .expect("renamed workflow");
    assert_eq!(renamed, "Major Incident Workflow");

    // Deactivate type → workflow.is_active ikut + map menyembunyikan type.
    let (status, _) = update_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), type_id)),
        Json(V1UpdateWorkItemType {
            is_active: Some(false),
            ..Default::default()
        }),
    )
    .await
    .expect("deactivate");
    assert_eq!(status, StatusCode::OK);
    let (active,): (bool,) = sqlx::query_as("SELECT is_active FROM workflows WHERE id = $1")
        .bind(workflow_id)
        .fetch_one(&st.pool)
        .await
        .expect("workflow active");
    assert!(!active);
    let (_, map) = workflow_map(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
    )
    .await
    .expect("map inactive");
    assert!(map["types"].as_array().unwrap().is_empty());

    // Delete type (detach dulu) → workflow + states + transitions soft-delete.
    let (status, _) = delete_project(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id, type_id)),
    )
    .await
    .expect("detach");
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = delete_workspace(State(st.clone()), AuthUser(owner), Path((slug.clone(), type_id)))
        .await
        .expect("delete type");
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (workflow_live, states_live, transitions_live): (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM workflows WHERE id = $1 AND deleted_at IS NULL), \
                (SELECT COUNT(*) FROM workflow_states WHERE workflow_id = $1 AND deleted_at IS NULL), \
                (SELECT COUNT(*) FROM workflow_transitions WHERE workflow_id = $1 AND deleted_at IS NULL)",
    )
    .bind(workflow_id)
    .fetch_one(&st.pool)
    .await
    .expect("cascade counts");
    assert_eq!(workflow_live, 0);
    assert_eq!(states_live, 0);
    assert_eq!(transitions_live, 0);

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn workflow_map_hides_inactive_types() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfhide").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    // Workflow + state, lalu type aktif yang di-enable ke project.
    let (_, wf) = create_workflow(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(WorkflowBody {
            name: Some("Hidden Type Workflow".into()),
            description: None,
            is_active: None,
        }),
    )
    .await
    .expect("workflow");
    let wf_id = Uuid::parse_str(wf["id"].as_str().unwrap()).unwrap();
    let _ = create_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), wf_id)),
        Json(WorkflowStateBody {
            name: Some("New".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("state");

    let (type_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workflow_id, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Type Hideable', '', '{}', false, false, true, 0, $1, $2, now(), now()) \
         RETURNING id",
    )
    .bind(wf_id)
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("type");
    let (status, _) = import_to_project(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
        Json(json!({"work_item_types": [type_id]})),
    )
    .await
    .expect("import");
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Aktif → muncul di map.
    let (status, map) = workflow_map(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
    )
    .await
    .expect("map active");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        map["types"].as_array().unwrap().len(),
        1,
        "type aktif harus muncul di map"
    );

    // Nonaktif → link/mirror tetap hidup, tapi type disembunyikan dari map.
    sqlx::query("UPDATE issue_types SET is_active = false WHERE id = $1")
        .bind(type_id)
        .execute(&st.pool)
        .await
        .expect("deactivate type");
    let (status, map) = workflow_map(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
    )
    .await
    .expect("map inactive");
    assert_eq!(status, StatusCode::OK);
    assert!(
        map["types"].as_array().unwrap().is_empty(),
        "type nonaktif tidak boleh muncul di map"
    );
    let (link_live, mirror_live): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM project_issue_types \
                 WHERE project_id = $1 AND issue_type_id = $2 AND deleted_at IS NULL), \
                (SELECT COUNT(*) FROM states \
                 WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_one(&st.pool)
    .await
    .expect("link/mirror after deactivate");
    assert_eq!(
        link_live, 1,
        "menonaktifkan type tidak boleh menghapus link"
    );
    assert_eq!(
        mirror_live, 1,
        "menonaktifkan type tidak boleh menghapus mirror"
    );

    // Aktif kembali → muncul lagi.
    sqlx::query("UPDATE issue_types SET is_active = true WHERE id = $1")
        .bind(type_id)
        .execute(&st.pool)
        .await
        .expect("reactivate type");
    let (status, map) = workflow_map(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
    )
    .await
    .expect("map reactivated");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        map["types"].as_array().unwrap().len(),
        1,
        "type yang diaktifkan kembali harus muncul di map"
    );

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn workflow_map_hides_epic_types() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfepic").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    // Workflow via the handler so the workspace owns a valid row.
    let (_, wf) = create_workflow(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(WorkflowBody {
            name: Some("Epic Type Workflow".into()),
            description: None,
            is_active: None,
        }),
    )
    .await
    .expect("workflow");
    let wf_id = Uuid::parse_str(wf["id"].as_str().unwrap()).unwrap();

    // Epic type, active and linked directly: `is_epic = true` is the only
    // reason it must stay out of the map (mirrors `p.is_epic = false`).
    let (type_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workflow_id, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Epic Type', '', '{}', true, false, true, 0, $1, $2, now(), now()) \
         RETURNING id",
    )
    .bind(wf_id)
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("epic type");
    sqlx::query(
        "INSERT INTO project_issue_types (id, issue_type_id, project_id, workspace_id, level, is_default, \
         created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, 0, false, now(), now())",
    )
    .bind(type_id)
    .bind(project_id)
    .bind(ws_id)
    .execute(&st.pool)
    .await
    .expect("epic link");

    let (status, map) = workflow_map(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
    )
    .await
    .expect("map with epic type");
    assert_eq!(status, StatusCode::OK);
    assert!(
        map["types"].as_array().unwrap().is_empty(),
        "type epic tidak boleh muncul di map"
    );

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn unlink_cross_workspace_and_switch_with_issues() {
    let st = app_state().await;
    let (slug_a, ws_a, project_a) = make_workspace(&st, "wfxa").await;
    let (slug_b, ws_b, project_b) = make_workspace(&st, "wfxb").await;
    let (owner_a,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug_a)
        .fetch_one(&st.pool)
        .await
        .expect("owner A");
    let (owner_b,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug_b)
        .fetch_one(&st.pool)
        .await
        .expect("owner B");

    // Workspace B: workflow + type enabled di project B.
    let (_, wf_b) = create_workflow(
        State(st.clone()),
        AuthUser(owner_b),
        Path(slug_b.clone()),
        Json(WorkflowBody {
            name: Some("B Workflow".into()),
            description: None,
            is_active: None,
        }),
    )
    .await
    .expect("B workflow");
    let wf_b_id = Uuid::parse_str(wf_b["id"].as_str().unwrap()).unwrap();
    let _ = create_state(
        State(st.clone()),
        AuthUser(owner_b),
        Path((slug_b.clone(), wf_b_id)),
        Json(WorkflowStateBody {
            name: Some("B New".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("B state");
    let (type_b,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workflow_id, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Type B', '', '{}', false, false, true, 0, $1, $2, now(), now()) \
         RETURNING id",
    )
    .bind(wf_b_id)
    .bind(ws_b)
    .fetch_one(&st.pool)
    .await
    .expect("type B");
    let (status, _) = import_to_project(
        State(st.clone()),
        AuthUser(owner_b),
        Path((slug_b.clone(), project_b)),
        Json(json!({"work_item_types": [type_b]})),
    )
    .await
    .expect("import B");
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Admin A tidak boleh men-unlink project B (cross-workspace) → 404.
    let (status, _) = unlink_type(
        State(st.clone()),
        AuthUser(owner_a),
        Path((slug_a.clone(), project_b, type_b)),
    )
    .await
    .expect("cross-workspace unlink");
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (b_link_live, b_mirror_live): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM project_issue_types \
                 WHERE project_id = $1 AND issue_type_id = $2 AND deleted_at IS NULL), \
                (SELECT COUNT(*) FROM states \
                 WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL)",
    )
    .bind(project_b)
    .bind(type_b)
    .fetch_one(&st.pool)
    .await
    .expect("B data");
    assert_eq!(b_link_live, 1, "data workspace B tidak boleh berubah");
    assert_eq!(b_mirror_live, 1);

    // Workspace A: dua workflow; type T pakai workflow A1, enabled di project A.
    let (_, wf_a1) = create_workflow(
        State(st.clone()),
        AuthUser(owner_a),
        Path(slug_a.clone()),
        Json(WorkflowBody {
            name: Some("A Workflow 1".into()),
            description: None,
            is_active: None,
        }),
    )
    .await
    .expect("A workflow 1");
    let wf_a1_id = Uuid::parse_str(wf_a1["id"].as_str().unwrap()).unwrap();
    let _ = create_state(
        State(st.clone()),
        AuthUser(owner_a),
        Path((slug_a.clone(), wf_a1_id)),
        Json(WorkflowStateBody {
            name: Some("A1 New".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("A1 state");
    let (_, wf_a2) = create_workflow(
        State(st.clone()),
        AuthUser(owner_a),
        Path(slug_a.clone()),
        Json(WorkflowBody {
            name: Some("A Workflow 2".into()),
            description: None,
            is_active: None,
        }),
    )
    .await
    .expect("A workflow 2");
    let wf_a2_id = Uuid::parse_str(wf_a2["id"].as_str().unwrap()).unwrap();
    let _ = create_state(
        State(st.clone()),
        AuthUser(owner_a),
        Path((slug_a.clone(), wf_a2_id)),
        Json(WorkflowStateBody {
            name: Some("A2 New".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("A2 state");
    let (type_t,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workflow_id, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Type T', '', '{}', false, false, true, 0, $1, $2, now(), now()) \
         RETURNING id",
    )
    .bind(wf_a1_id)
    .bind(ws_a)
    .fetch_one(&st.pool)
    .await
    .expect("type T");
    let (status, _) = import_to_project(
        State(st.clone()),
        AuthUser(owner_a),
        Path((slug_a.clone(), project_a)),
        Json(json!({"work_item_types": [type_t]})),
    )
    .await
    .expect("import T");
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Live issue bertipe T memblokir switch workflow.
    let (state_id,): (Uuid,) = sqlx::query_as(
        "SELECT id FROM states WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL LIMIT 1",
    )
    .bind(project_a)
    .bind(type_t)
    .fetch_one(&st.pool)
    .await
    .expect("T mirror");
    let (issue_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issues (id, name, description_html, description_json, priority, is_draft, \
         sort_order, sequence_id, state_id, type_id, project_id, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Live work', '<p></p>', '{}', 'none', true, \
         65535, (SELECT COALESCE(MAX(sequence_id), 0) + 1000 FROM issues WHERE project_id = $2), \
         $1, $4, $2, $3, now(), now()) RETURNING id",
    )
    .bind(state_id)
    .bind(project_a)
    .bind(ws_a)
    .bind(type_t)
    .fetch_one(&st.pool)
    .await
    .expect("live issue");

    let (status, body) = update_workspace(
        State(st.clone()),
        AuthUser(owner_a),
        Path((slug_a.clone(), type_t)),
        Json(V1UpdateWorkItemType {
            workflow: Some(Some(wf_a2_id)),
            ..Default::default()
        }),
    )
    .await
    .expect("switch with live issues");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"],
        "Cannot change the workflow while the type has live work items"
    );

    // Project-scope DELETE: 400 saat issue hidup, 204 setelah issue ditutup.
    let (status, body) = delete_project(
        State(st.clone()),
        AuthUser(owner_a),
        Path((slug_a.clone(), project_a, type_t)),
    )
    .await
    .expect("delete in use");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Type is in use by work items");

    sqlx::query("UPDATE issues SET deleted_at = now() WHERE id = $1")
        .bind(issue_id)
        .execute(&st.pool)
        .await
        .expect("close issue");

    let (status, _) = delete_project(
        State(st.clone()),
        AuthUser(owner_a),
        Path((slug_a.clone(), project_a, type_t)),
    )
    .await
    .expect("detach T");
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (link_live, mirror_live): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM project_issue_types \
                 WHERE project_id = $1 AND issue_type_id = $2 AND deleted_at IS NULL), \
                (SELECT COUNT(*) FROM states \
                 WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL)",
    )
    .bind(project_a)
    .bind(type_t)
    .fetch_one(&st.pool)
    .await
    .expect("detached T");
    assert_eq!(link_live, 0);
    assert_eq!(mirror_live, 0);
    let (status, _) = delete_project(
        State(st.clone()),
        AuthUser(owner_a),
        Path((slug_a.clone(), project_a, type_t)),
    )
    .await
    .expect("detach T again");
    assert_eq!(status, StatusCode::NO_CONTENT, "delete kedua idempotent");

    // Epic tidak boleh punya workflow.
    let (status, body) = create_workspace(
        State(st.clone()),
        AuthUser(owner_a),
        Path(slug_a.clone()),
        Json(V1CreateWorkItemType {
            name: Some("Epic Type".into()),
            is_epic: Some(true),
            workflow: Some(wf_a1_id),
            ..Default::default()
        }),
    )
    .await
    .expect("epic with workflow");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Epic types cannot have a workflow");

    purge(&st.pool, &slug_b).await;
    purge(&st.pool, &slug_a).await;
}

#[tokio::test]
async fn ensure_workflow_for_type_adopts_and_defaults() {
    let st = app_state().await;
    let (slug, ws_id, _project_id) = make_workspace(&st, "wfwf").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");
    let mut conn = st.pool.acquire().await.expect("conn");

    let first = ensure_workflow_for_type(&mut conn, ws_id, "Incident", true, owner)
        .await
        .expect("ensure workflow");
    let (name,): (String,) = sqlx::query_as("SELECT name FROM workflows WHERE id = $1")
        .bind(first)
        .fetch_one(&mut *conn)
        .await
        .expect("workflow row");
    assert_eq!(name, "Incident Workflow");
    let (defaults,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM workflow_states WHERE workflow_id = $1 AND deleted_at IS NULL AND is_default",
    )
    .bind(first)
    .fetch_one(&mut *conn)
    .await
    .expect("default state");
    assert_eq!(defaults, 1);

    // Idempotent: panggilan kedua mengadopsi workflow yang sama (masih orphan).
    let second = ensure_workflow_for_type(&mut conn, ws_id, "Incident", true, owner)
        .await
        .expect("ensure again");
    assert_eq!(first, second);

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn sync_type_workflow_renames_when_free_and_syncs_active() {
    let st = app_state().await;
    let (slug, ws_id, _project_id) = make_workspace(&st, "wfsync").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");
    let mut conn = st.pool.acquire().await.expect("conn");

    let incident_wf = ensure_workflow_for_type(&mut conn, ws_id, "Incident", true, owner)
        .await
        .expect("incident workflow");
    sync_type_workflow(&mut conn, ws_id, "Major Incident", false, incident_wf)
        .await
        .expect("sync");
    let (name, active): (String, bool) =
        sqlx::query_as("SELECT name, is_active FROM workflows WHERE id = $1")
            .bind(incident_wf)
            .fetch_one(&mut *conn)
            .await
            .expect("workflow row");
    assert_eq!(name, "Major Incident Workflow");
    assert!(!active);

    // Nama derived sudah dipakai workflow lain → nama lama dipertahankan.
    let request_wf = ensure_workflow_for_type(&mut conn, ws_id, "Request", true, owner)
        .await
        .expect("request workflow");
    sync_type_workflow(&mut conn, ws_id, "Major Incident", true, request_wf)
        .await
        .expect("sync collision");
    let (request_name,): (String,) = sqlx::query_as("SELECT name FROM workflows WHERE id = $1")
        .bind(request_wf)
        .fetch_one(&mut *conn)
        .await
        .expect("request row");
    assert_eq!(request_name, "Request Workflow");

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn soft_delete_workflow_cascade_clears_states_and_transitions() {
    let st = app_state().await;
    let (slug, ws_id, _project_id) = make_workspace(&st, "wfdel").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");
    let mut conn = st.pool.acquire().await.expect("conn");

    let workflow_id = ensure_workflow_for_type(&mut conn, ws_id, "Incident", true, owner)
        .await
        .expect("workflow");
    let (_, progress) = create_state(
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
    .expect("state");
    let progress_id = Uuid::parse_str(progress["id"].as_str().unwrap()).unwrap();
    let (_, states) = list_states(State(st.clone()), AuthUser(owner), Path((slug.clone(), workflow_id)))
        .await
        .expect("states");
    let default_id = Uuid::parse_str(states[0]["id"].as_str().unwrap()).unwrap();
    let _ = create_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(TransitionBody {
            from_state_id: Some(default_id),
            to_state_id: Some(progress_id),
        }),
    )
    .await
    .expect("transition");

    soft_delete_workflow_cascade(&mut conn, workflow_id)
        .await
        .expect("cascade");
    let (workflow_live, states_live, transitions_live): (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM workflows WHERE id = $1 AND deleted_at IS NULL), \
                (SELECT COUNT(*) FROM workflow_states WHERE workflow_id = $1 AND deleted_at IS NULL), \
                (SELECT COUNT(*) FROM workflow_transitions WHERE workflow_id = $1 AND deleted_at IS NULL)",
    )
    .bind(workflow_id)
    .fetch_one(&mut *conn)
    .await
    .expect("cascade counts");
    assert_eq!(workflow_live, 0);
    assert_eq!(states_live, 0);
    assert_eq!(transitions_live, 0);

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn create_type_autocreates_workflow_and_rejects_explicit() {
    let st = app_state().await;
    let (slug, _ws_id, _project_id) = make_workspace(&st, "wfcreate").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let (status, created) = create_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(V1CreateWorkItemType {
            name: Some("Incident".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("create type");
    assert_eq!(status, StatusCode::CREATED);
    let ws_id: Uuid = sqlx::query_scalar("SELECT id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("workspace id");
    let workflow_id = Uuid::parse_str(created["workflow"].as_str().expect("workflow id")).unwrap();
    let (name,): (String,) = sqlx::query_as("SELECT name FROM workflows WHERE id = $1")
        .bind(workflow_id)
        .fetch_one(&st.pool)
        .await
        .expect("workflow row");
    assert_eq!(name, "Incident Workflow");
    let (defaults,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM workflow_states WHERE workflow_id = $1 AND deleted_at IS NULL AND is_default",
    )
    .bind(workflow_id)
    .fetch_one(&st.pool)
    .await
    .expect("default state");
    assert_eq!(defaults, 1);

    // Body `workflow` eksplisit ditolak dan tidak meninggalkan type orphan.
    let (status, body) = create_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(V1CreateWorkItemType {
            name: Some("Problem".into()),
            workflow: Some(workflow_id),
            ..Default::default()
        }),
    )
    .await
    .expect("reject explicit workflow");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Workflows are managed through work item types");
    let (problem_types,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM issue_types WHERE workspace_id = $1 AND name = 'Problem' AND deleted_at IS NULL",
    )
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("rejected type rows");
    assert_eq!(problem_types, 0);

    // Epic tetap tanpa workflow.
    let (status, epic) = create_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(V1CreateWorkItemType {
            name: Some("Epic Thing".into()),
            is_epic: Some(true),
            ..Default::default()
        }),
    )
    .await
    .expect("create epic");
    assert_eq!(status, StatusCode::CREATED);
    assert!(epic["workflow"].is_null());

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn update_type_rejects_workflow_and_syncs_derived_name() {
    let st = app_state().await;
    let (slug, _ws_id, _project_id) = make_workspace(&st, "wfupdate").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let (_, created) = create_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(V1CreateWorkItemType {
            name: Some("Incident".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("create");
    let type_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    let workflow_id = Uuid::parse_str(created["workflow"].as_str().unwrap()).unwrap();

    // Body `workflow` (null eksplisit) ditolak.
    let (status, body) = update_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), type_id)),
        Json(V1UpdateWorkItemType {
            workflow: Some(None),
            ..Default::default()
        }),
    )
    .await
    .expect("reject workflow body");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Workflows are managed through work item types");

    // Rename type → workflow ikut rename.
    let (status, _) = update_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), type_id)),
        Json(V1UpdateWorkItemType {
            name: Some("Major Incident".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("rename");
    assert_eq!(status, StatusCode::OK);
    let (renamed,): (String,) = sqlx::query_as("SELECT name FROM workflows WHERE id = $1")
        .bind(workflow_id)
        .fetch_one(&st.pool)
        .await
        .expect("workflow name");
    assert_eq!(renamed, "Major Incident Workflow");

    // Toggle active → workflow.is_active ikut.
    let (status, _) = update_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), type_id)),
        Json(V1UpdateWorkItemType {
            is_active: Some(false),
            ..Default::default()
        }),
    )
    .await
    .expect("deactivate");
    assert_eq!(status, StatusCode::OK);
    let (active,): (bool,) = sqlx::query_as("SELECT is_active FROM workflows WHERE id = $1")
        .bind(workflow_id)
        .fetch_one(&st.pool)
        .await
        .expect("workflow active");
    assert!(!active);

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn delete_type_cascades_workflow() {
    let st = app_state().await;
    let (slug, _ws_id, _project_id) = make_workspace(&st, "wfdel2").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let (_, created) = create_workspace(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.clone()),
        Json(V1CreateWorkItemType {
            name: Some("Incident".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("create");
    let type_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    let workflow_id = Uuid::parse_str(created["workflow"].as_str().unwrap()).unwrap();

    let (_, progress) = create_state(
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
    .expect("state");
    let progress_id = Uuid::parse_str(progress["id"].as_str().unwrap()).unwrap();
    let (_, states) = list_states(State(st.clone()), AuthUser(owner), Path((slug.clone(), workflow_id)))
        .await
        .expect("states");
    let default_id = Uuid::parse_str(states[0]["id"].as_str().unwrap()).unwrap();
    let _ = create_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(TransitionBody {
            from_state_id: Some(default_id),
            to_state_id: Some(progress_id),
        }),
    )
    .await
    .expect("transition");

    let (status, _) = delete_workspace(State(st.clone()), AuthUser(owner), Path((slug.clone(), type_id)))
        .await
        .expect("delete type");
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (workflow_live, states_live, transitions_live): (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM workflows WHERE id = $1 AND deleted_at IS NULL), \
                (SELECT COUNT(*) FROM workflow_states WHERE workflow_id = $1 AND deleted_at IS NULL), \
                (SELECT COUNT(*) FROM workflow_transitions WHERE workflow_id = $1 AND deleted_at IS NULL)",
    )
    .bind(workflow_id)
    .fetch_one(&st.pool)
    .await
    .expect("cascade counts");
    assert_eq!(workflow_live, 0);
    assert_eq!(states_live, 0);
    assert_eq!(transitions_live, 0);

    purge(&st.pool, &slug).await;
}
