use api::middleware::auth::AuthUser;
use api::routes::draft::{
    create as draft_create, create_draft_to_issue, ConvertBody, CreateDraftBody,
};
use api::routes::issue_common::resolve_issue_state;
use api::routes::issue_update::{patch_issue, PatchIssue};
use api::routes::issue_write::{create as create_issue, CreateIssue};
use api::routes::v1::work_item::{create as v1_create, update as v1_update, V1WriteWorkItem};
use api::routes::v1::work_item_type::import_to_project;
use api::routes::workflow::{
    create_state, create_transition, create_workflow, evaluate_transition, TransitionBody,
    TransitionContext, WorkflowBody, WorkflowStateBody,
};
use api::routes::workspace::create;
use api::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use common::config::AppConfig;
use serde_json::{json, Value};
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
        "DELETE FROM issue_description_versions WHERE workspace_id = $1",
        "DELETE FROM issue_subscribers WHERE workspace_id = $1",
        "DELETE FROM issue_sequences WHERE workspace_id = $1",
        "DELETE FROM draft_issues WHERE workspace_id = $1",
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

    // Regresi: fallback legacy dulu tidak memfilter `type_id IS NULL`, sehingga
    // mirror typed ber-sequence lebih kecil menang untuk issue untyped.
    let (low_type,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Problem', '', '{}', false, false, true, 0, $1, now(), now()) \
         RETURNING id",
    )
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("low-seq type");
    let (low_typed_state,): (Uuid,) = sqlx::query_as(
        "INSERT INTO states (id, name, description, color, slug, sequence, \"group\", is_triage, \
         \"default\", project_id, workspace_id, type_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'New', '', '#60646C', 'new', 5000, 'backlog', false, true, \
         $1, $2, $3, now(), now()) RETURNING id",
    )
    .bind(project_id)
    .bind(ws_id)
    .bind(low_type)
    .fetch_one(&st.pool)
    .await
    .expect("low-seq typed state");

    let resolved = resolve_issue_state(&st.pool, project_id, None, None)
        .await
        .expect("resolve untyped vs low-seq typed default");
    assert_eq!(resolved, Some(legacy_default));
    assert_ne!(resolved, Some(low_typed_state));

    // Type epic: lookup typed menolak `is_epic`, fallback legacy tetap untyped.
    let (epic_type,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Epic', '', '{}', true, false, true, 0, $1, now(), now()) \
         RETURNING id",
    )
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("epic type");
    let (epic_state,): (Uuid,) = sqlx::query_as(
        "INSERT INTO states (id, name, description, color, slug, sequence, \"group\", is_triage, \
         \"default\", project_id, workspace_id, type_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'New', '', '#60646C', 'new', 6000, 'backlog', false, true, \
         $1, $2, $3, now(), now()) RETURNING id",
    )
    .bind(project_id)
    .bind(ws_id)
    .bind(epic_type)
    .fetch_one(&st.pool)
    .await
    .expect("epic typed state");

    let resolved = resolve_issue_state(&st.pool, project_id, Some(epic_type), None)
        .await
        .expect("resolve epic");
    assert_eq!(resolved, Some(legacy_default));
    assert_ne!(resolved, Some(epic_state));

    // Type dengan HANYA state typed non-default: state typed pertama menang,
    // legacy tidak pernah dipakai selama type punya state hidup.
    let (first_type,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Task', '', '{}', false, false, true, 0, $1, now(), now()) \
         RETURNING id",
    )
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("typed-first type");
    let (first_typed_state,): (Uuid,) = sqlx::query_as(
        "INSERT INTO states (id, name, description, color, slug, sequence, \"group\", is_triage, \
         \"default\", project_id, workspace_id, type_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'New', '', '#60646C', 'new', 4000, 'backlog', false, false, \
         $1, $2, $3, now(), now()) RETURNING id",
    )
    .bind(project_id)
    .bind(ws_id)
    .bind(first_type)
    .fetch_one(&st.pool)
    .await
    .expect("typed-first state");

    let resolved = resolve_issue_state(&st.pool, project_id, Some(first_type), None)
        .await
        .expect("resolve typed-first");
    assert_eq!(resolved, Some(first_typed_state));
    assert_ne!(resolved, Some(legacy_default));

    let _ = owner;
    purge(&st.pool, &slug).await;
}

// --- DB-backed PATCH enforcement test --------------------------------------

#[tokio::test]
async fn patch_rejects_disallowed_transition() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfpatch").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    // Workflow W dengan state New (default), In Progress, Closed.
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

    let (_, new_state) = create_state(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(WorkflowStateBody {
            name: Some("New".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("state New");
    let workflow_new_id =
        Uuid::parse_str(new_state["id"].as_str().expect("new state id")).expect("uuid");

    let (_, progress_state) = create_state(
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
    .expect("state In Progress");
    let workflow_progress_id =
        Uuid::parse_str(progress_state["id"].as_str().expect("progress id")).expect("uuid");

    let (_, closed_state) = create_state(
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
    .expect("state Closed");
    let workflow_closed_id =
        Uuid::parse_str(closed_state["id"].as_str().expect("closed id")).expect("uuid");

    // Hanya New → In Progress yang terdaftar; New → Closed tidak.
    let (status, _) = create_transition(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), workflow_id)),
        Json(TransitionBody {
            from_state_id: Some(workflow_new_id),
            to_state_id: Some(workflow_progress_id),
        }),
    )
    .await
    .expect("transition");
    assert_eq!(status, StatusCode::CREATED);
    let (closed_pairs,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM workflow_transitions WHERE workflow_id = $1 \
         AND from_state_id = $2 AND to_state_id = $3 AND deleted_at IS NULL",
    )
    .bind(workflow_id)
    .bind(workflow_new_id)
    .bind(workflow_closed_id)
    .fetch_one(&st.pool)
    .await
    .expect("closed pair count");
    assert_eq!(closed_pairs, 0, "New → Closed tidak boleh terdaftar");

    // Type T memakai workflow W, lalu di-enable di project (materialize mirror).
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

    let mirrors: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, name FROM states WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_all(&st.pool)
    .await
    .expect("mirrors");
    let mirror_of = |name: &str| {
        mirrors
            .iter()
            .find(|(_, n)| n == name)
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("missing mirror state {name}"))
    };
    let new_state_id = mirror_of("New");
    let in_progress_state_id = mirror_of("In Progress");
    let closed_state_id = mirror_of("Closed");

    // Issue typed dengan state New, dibuat lewat handler create publik.
    let (status, Json(issue)) = create_issue(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
        Json(CreateIssue {
            name: "Server down".into(),
            assignee_ids: None,
            label_ids: None,
            state_id: Some(new_state_id),
            description_html: None,
            priority: None,
            start_date: None,
            target_date: None,
            parent_id: None,
            type_id: Some(type_id),
            estimate_point: None,
        }),
    )
    .await
    .expect("issue create");
    assert_eq!(status, StatusCode::CREATED);
    let issue_id = Uuid::parse_str(issue["id"].as_str().expect("issue id")).expect("uuid");

    // New → Closed tidak terjangkau dari workflow type → 400 + allowed In Progress.
    let (updated_before,): (String,) =
        sqlx::query_as("SELECT updated_at::text FROM issues WHERE id = $1")
            .bind(issue_id)
            .fetch_one(&st.pool)
            .await
            .expect("updated_at before denial");
    let (status, body) = patch_issue(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id, issue_id)),
        Json(PatchIssue {
            state_id: Some(Some(closed_state_id)),
            ..Default::default()
        }),
    )
    .await
    .expect("patch denied");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid state transition");
    assert_eq!(
        body["allowed_state_ids"],
        json!([in_progress_state_id.to_string()])
    );
    let (stored, updated_after): (Option<Uuid>, String) =
        sqlx::query_as("SELECT state_id, updated_at::text FROM issues WHERE id = $1")
            .bind(issue_id)
            .fetch_one(&st.pool)
            .await
            .expect("state after denial");
    assert_eq!(stored, Some(new_state_id), "state tidak boleh berubah");
    assert_eq!(
        updated_after, updated_before,
        "denied patch tidak boleh menyentuh row"
    );

    // New → In Progress terdaftar → lolos.
    let (status, _) = patch_issue(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id, issue_id)),
        Json(PatchIssue {
            state_id: Some(Some(in_progress_state_id)),
            ..Default::default()
        }),
    )
    .await
    .expect("patch allowed");
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (stored,): (Option<Uuid>,) = sqlx::query_as("SELECT state_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_one(&st.pool)
        .await
        .expect("state after patch");
    assert_eq!(stored, Some(in_progress_state_id));

    // --- Current state NULL: hanya default type yang boleh dipilih ---------
    sqlx::query("UPDATE issues SET state_id = NULL WHERE id = $1")
        .bind(issue_id)
        .execute(&st.pool)
        .await
        .expect("clear state");

    // State typed non-default ditolak; allowed hanya default New.
    let (status, body) = patch_issue(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id, issue_id)),
        Json(PatchIssue {
            state_id: Some(Some(in_progress_state_id)),
            ..Default::default()
        }),
    )
    .await
    .expect("patch from null denied");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid state transition");
    assert_eq!(body["allowed_state_ids"], json!([new_state_id.to_string()]));
    let (stored,): (Option<Uuid>,) = sqlx::query_as("SELECT state_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_one(&st.pool)
        .await
        .expect("state after null denial");
    assert_eq!(stored, None, "state NULL tidak boleh berubah");

    // Patch tanpa state_id menyelesaikan default typed; activity memakai
    // state hasil resolusi, bukan body.state_id yang kosong.
    let (status, _) = patch_issue(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id, issue_id)),
        Json(PatchIssue {
            priority: Some(Some("low".into())),
            ..Default::default()
        }),
    )
    .await
    .expect("patch resolves default");
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (stored,): (Option<Uuid>,) = sqlx::query_as("SELECT state_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_one(&st.pool)
        .await
        .expect("state after resolve");
    assert_eq!(stored, Some(new_state_id));
    let (resolved_rows,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM issue_activities WHERE issue_id = $1 AND field = 'state' \
         AND old_identifier IS NULL AND new_identifier = $2",
    )
    .bind(issue_id)
    .bind(new_state_id)
    .fetch_one(&st.pool)
    .await
    .expect("resolved state activity");
    assert_eq!(resolved_rows, 1, "state NULL → default harus tercatat");

    purge(&st.pool, &slug).await;
}

// --- DB-backed create + type-change + draft-convert coverage (C4) ----------

/// Type hasil workflow materialization + map nama state mirror → id.
struct WorkflowType {
    type_id: Uuid,
    states: std::collections::HashMap<String, Uuid>,
}

impl WorkflowType {
    fn state(&self, name: &str) -> Uuid {
        *self
            .states
            .get(name)
            .unwrap_or_else(|| panic!("missing mirror state {name}"))
    }
}

/// Buat workflow + states + transisi, type yang memakainya, lalu import ke
/// project (materialize mirror). State pertama otomatis jadi default
/// (`create_state`, workflow.rs:791-795).
async fn make_workflow_type(
    st: &AppState,
    slug: &str,
    ws_id: Uuid,
    owner: Uuid,
    project_id: Uuid,
    type_name: &str,
    states: &[(&str, &str)],
    transitions: &[(&str, &str)],
) -> WorkflowType {
    let (_, created) = create_workflow(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.to_string()),
        Json(WorkflowBody {
            name: Some(format!("{type_name} Workflow")),
            description: None,
            is_active: None,
        }),
    )
    .await
    .expect("workflow");
    let workflow_id = Uuid::parse_str(created["id"].as_str().expect("workflow id")).expect("uuid");

    let mut workflow_states = std::collections::HashMap::new();
    for (name, group) in states {
        let (status, body) = create_state(
            State(st.clone()),
            AuthUser(owner),
            Path((slug.to_string(), workflow_id)),
            Json(WorkflowStateBody {
                name: Some((*name).to_string()),
                group: Some((*group).to_string()),
                ..Default::default()
            }),
        )
        .await
        .expect("workflow state");
        assert_eq!(status, StatusCode::CREATED, "state {name}");
        workflow_states.insert(
            (*name).to_string(),
            Uuid::parse_str(body["id"].as_str().expect("state id")).expect("uuid"),
        );
    }
    for (from, to) in transitions {
        let (status, _) = create_transition(
            State(st.clone()),
            AuthUser(owner),
            Path((slug.to_string(), workflow_id)),
            Json(TransitionBody {
                from_state_id: Some(workflow_states[*from]),
                to_state_id: Some(workflow_states[*to]),
            }),
        )
        .await
        .expect("transition");
        assert_eq!(status, StatusCode::CREATED, "transition {from} -> {to}");
    }

    let (type_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workflow_id, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, '', '{}', false, false, true, 0, $2, $3, now(), now()) \
         RETURNING id",
    )
    .bind(type_name)
    .bind(workflow_id)
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("type");
    let (status, _) = import_to_project(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.to_string(), project_id)),
        Json(json!({"work_item_types": [type_id]})),
    )
    .await
    .expect("import");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let mirrors: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, name FROM states WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(type_id)
    .fetch_all(&st.pool)
    .await
    .expect("mirror states");
    let states: std::collections::HashMap<String, Uuid> =
        mirrors.into_iter().map(|(id, name)| (name, id)).collect();
    WorkflowType { type_id, states }
}

async fn insert_issue_type(st: &AppState, ws_id: Uuid, name: &str, is_epic: bool) -> Uuid {
    let (type_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, is_active, \
         level, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, '', '{}', $2, false, true, 0, $3, now(), now()) \
         RETURNING id",
    )
    .bind(name)
    .bind(is_epic)
    .bind(ws_id)
    .fetch_one(&st.pool)
    .await
    .expect("issue type");
    type_id
}

async fn insert_typed_state(
    st: &AppState,
    project_id: Uuid,
    ws_id: Uuid,
    type_id: Uuid,
    name: &str,
    slug: &str,
    sequence: f64,
    group: &str,
    is_default: bool,
) -> Uuid {
    let (state_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO states (id, name, description, color, slug, sequence, \"group\", is_triage, \
         \"default\", project_id, workspace_id, type_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, '', '#60646C', $2, $3, $4, false, $5, $6, $7, $8, now(), now()) \
         RETURNING id",
    )
    .bind(name)
    .bind(slug)
    .bind(sequence)
    .bind(group)
    .bind(is_default)
    .bind(project_id)
    .bind(ws_id)
    .bind(type_id)
    .fetch_one(&st.pool)
    .await
    .expect("typed state");
    state_id
}

/// State legacy (type_id NULL) + opsi soft-deleted; `group="triage"` juga
/// menandai `is_triage`.
async fn insert_legacy_state(
    st: &AppState,
    project_id: Uuid,
    ws_id: Uuid,
    name: &str,
    slug: &str,
    sequence: f64,
    group: &str,
    deleted: bool,
) -> Uuid {
    let (state_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO states (id, name, description, color, slug, sequence, \"group\", is_triage, \
         \"default\", project_id, workspace_id, created_at, updated_at, deleted_at) \
         VALUES (gen_random_uuid(), $1, '', '#60646C', $2, $3, $4, $5, false, $6, $7, now(), now(), \
         CASE WHEN $8 THEN now() ELSE NULL END) RETURNING id",
    )
    .bind(name)
    .bind(slug)
    .bind(sequence)
    .bind(group)
    .bind(group == "triage")
    .bind(project_id)
    .bind(ws_id)
    .bind(deleted)
    .fetch_one(&st.pool)
    .await
    .expect("legacy state");
    state_id
}

async fn create_issue_id(
    st: &AppState,
    owner: Uuid,
    slug: &str,
    project_id: Uuid,
    name: &str,
    type_id: Option<Uuid>,
    state_id: Option<Uuid>,
) -> Uuid {
    let (status, Json(issue)) = create_issue(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.to_string(), project_id)),
        Json(CreateIssue {
            name: name.to_string(),
            type_id,
            state_id,
            ..Default::default()
        }),
    )
    .await
    .expect("issue create");
    assert_eq!(status, StatusCode::CREATED, "create {name}");
    Uuid::parse_str(issue["id"].as_str().expect("issue id")).expect("uuid")
}

async fn issue_type_state(st: &AppState, issue_id: Uuid) -> (Option<Uuid>, Option<Uuid>) {
    sqlx::query_as("SELECT type_id, state_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_one(&st.pool)
        .await
        .expect("issue row")
}

async fn state_activity_count(st: &AppState, issue_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_activities WHERE issue_id = $1 AND field = 'state'",
    )
    .bind(issue_id)
    .fetch_one(&st.pool)
    .await
    .expect("state activity count")
}

fn patch_body(type_id: Option<Option<Uuid>>, state_id: Option<Option<Uuid>>) -> PatchIssue {
    PatchIssue {
        type_id,
        state_id,
        ..Default::default()
    }
}

async fn patch_issue_req(
    st: &AppState,
    owner: Uuid,
    slug: &str,
    project_id: Uuid,
    issue_id: Uuid,
    body: PatchIssue,
) -> (StatusCode, Value) {
    let (status, Json(payload)) = patch_issue(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.to_string(), project_id, issue_id)),
        Json(body),
    )
    .await
    .expect("patch");
    (status, payload)
}

#[tokio::test]
async fn create_rejects_state_from_other_type() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfcreate").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let type_a = insert_issue_type(&st, ws_id, "Incident", false).await;
    let state_a = insert_typed_state(
        &st, project_id, ws_id, type_a, "New", "new", 10000.0, "backlog", true,
    )
    .await;
    let type_b = insert_issue_type(&st, ws_id, "Problem", false).await;
    let state_b = insert_typed_state(
        &st, project_id, ws_id, type_b, "TriageB", "triage-b", 10000.0, "backlog", true,
    )
    .await;

    // Kedua type di-enable di project.
    for t in [type_a, type_b] {
        let (status, _) = import_to_project(
            State(st.clone()),
            AuthUser(owner),
            Path((slug.clone(), project_id)),
            Json(json!({"work_item_types": [t]})),
        )
        .await
        .expect("import");
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    // State milik type B dengan body type A → 400, bukan tersimpan.
    let (status, body) = create_issue(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
        Json(CreateIssue {
            name: "Server down".into(),
            state_id: Some(state_b),
            type_id: Some(type_a),
            ..Default::default()
        }),
    )
    .await
    .expect("create cross-type state");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "State is not valid for this work item type");
    // Seed workspace membuat issue onboarding; yang dicek hanya issue baru
    // bertipe A (tidak boleh ada).
    let (persisted,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM issues WHERE project_id = $1 AND type_id = $2")
            .bind(project_id)
            .bind(type_a)
            .fetch_one(&st.pool)
            .await
            .expect("persisted issues");
    assert_eq!(
        persisted, 0,
        "create cross-type tidak boleh menyimpan issue"
    );

    // Tanpa state → default milik type A (typed default menang).
    let issue_id = create_issue_id(
        &st,
        owner,
        &slug,
        project_id,
        "Server up",
        Some(type_a),
        None,
    )
    .await;
    let (stored_type, stored_state) = issue_type_state(&st, issue_id).await;
    assert_eq!(stored_type, Some(type_a));
    assert_eq!(stored_state, Some(state_a));

    // Aturan amandemen #7: state legacy (untyped, live) eksplisit tetap sah
    // untuk work item bertipe.
    let legacy = insert_legacy_state(
        &st,
        project_id,
        ws_id,
        "Legacy Backlog",
        "legacy-backlog",
        50000.0,
        "backlog",
        false,
    )
    .await;
    let legacy_issue = create_issue_id(
        &st,
        owner,
        &slug,
        project_id,
        "legacy typed",
        Some(type_a),
        Some(legacy),
    )
    .await;
    let (stored_type, stored_state) = issue_type_state(&st, legacy_issue).await;
    assert_eq!(stored_type, Some(type_a));
    assert_eq!(stored_state, Some(legacy));

    // Predikat create sengaja lebih ketat dari Django `all_state_objects`:
    // state soft-deleted / triage ditolak dengan pesan lama.
    let deleted = insert_legacy_state(
        &st, project_id, ws_id, "Gone", "gone", 50001.0, "backlog", true,
    )
    .await;
    let triage = insert_legacy_state(
        &st,
        project_id,
        ws_id,
        "Triage Probe",
        "triage-probe",
        50002.0,
        "triage",
        false,
    )
    .await;
    for bad_state in [deleted, triage] {
        let (status, body) = create_issue(
            State(st.clone()),
            AuthUser(owner),
            Path((slug.clone(), project_id)),
            Json(CreateIssue {
                name: "bad state".into(),
                state_id: Some(bad_state),
                type_id: Some(type_a),
                ..Default::default()
            }),
        )
        .await
        .expect("create bad state");
        assert_eq!(status, StatusCode::BAD_REQUEST, "state {bad_state}");
        assert_eq!(
            body["error"], "State is not valid please pass a valid state_id",
            "state {bad_state}"
        );
    }

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn patch_type_change_reconciles_state_and_enforces() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wftc").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let a = make_workflow_type(
        &st,
        &slug,
        ws_id,
        owner,
        project_id,
        "Incident",
        &[
            ("New", "backlog"),
            ("In Progress", "started"),
            ("Closed", "completed"),
        ],
        &[("New", "In Progress"), ("In Progress", "Closed")],
    )
    .await;
    let b = make_workflow_type(
        &st,
        &slug,
        ws_id,
        owner,
        project_id,
        "Service Request",
        &[("Todo", "backlog"), ("Done", "completed")],
        &[("Todo", "Done")],
    )
    .await;

    // Type change + state milik type LAMA → 400, row tidak berubah.
    let issue_id = create_issue_id(
        &st,
        owner,
        &slug,
        project_id,
        "typed one",
        Some(a.type_id),
        Some(a.state("New")),
    )
    .await;
    let (status, body) = patch_issue_req(
        &st,
        owner,
        &slug,
        project_id,
        issue_id,
        patch_body(Some(Some(b.type_id)), Some(Some(a.state("New")))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "State is not valid for this work item type");
    let (stored_type, stored_state) = issue_type_state(&st, issue_id).await;
    assert_eq!(stored_type, Some(a.type_id));
    assert_eq!(stored_state, Some(a.state("New")));

    // Type change + state valid milik type BARU: tanpa cek transisi (B.Done
    // tidak terjangkau dari workflow A, jadi ini hanya lolos karena skip).
    let (status, _) = patch_issue_req(
        &st,
        owner,
        &slug,
        project_id,
        issue_id,
        patch_body(Some(Some(b.type_id)), Some(Some(b.state("Done")))),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (stored_type, stored_state) = issue_type_state(&st, issue_id).await;
    assert_eq!(stored_type, Some(b.type_id));
    assert_eq!(stored_state, Some(b.state("Done")));

    // Explicit same-state: no-op, tidak menambah activity `state`.
    let activities_before = state_activity_count(&st, issue_id).await;
    let (status, _) = patch_issue_req(
        &st,
        owner,
        &slug,
        project_id,
        issue_id,
        patch_body(None, Some(Some(b.state("Done")))),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        state_activity_count(&st, issue_id).await,
        activities_before,
        "same-state tidak boleh menulis activity state"
    );

    // Type change tanpa state → default type baru + activity `state` dari
    // auto-move.
    let moved_id = create_issue_id(
        &st,
        owner,
        &slug,
        project_id,
        "typed two",
        Some(a.type_id),
        Some(a.state("New")),
    )
    .await;
    let (status, _) = patch_issue_req(
        &st,
        owner,
        &slug,
        project_id,
        moved_id,
        patch_body(Some(Some(b.type_id)), None),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (stored_type, stored_state) = issue_type_state(&st, moved_id).await;
    assert_eq!(stored_type, Some(b.type_id));
    assert_eq!(stored_state, Some(b.state("Todo")));
    let (auto_moves,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM issue_activities WHERE issue_id = $1 AND field = 'state' \
         AND old_identifier = $2 AND new_identifier = $3",
    )
    .bind(moved_id)
    .bind(a.state("New"))
    .bind(b.state("Todo"))
    .fetch_one(&st.pool)
    .await
    .expect("auto-move activity");
    assert_eq!(
        auto_moves, 1,
        "type-change auto-move harus menulis activity state"
    );

    // Target cross-type pada patch type sama → 400 allowed_state_ids [].
    let cross_id = create_issue_id(
        &st,
        owner,
        &slug,
        project_id,
        "typed three",
        Some(a.type_id),
        Some(a.state("New")),
    )
    .await;
    let (status, body) = patch_issue_req(
        &st,
        owner,
        &slug,
        project_id,
        cross_id,
        patch_body(None, Some(Some(b.state("Todo")))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid state transition");
    assert_eq!(body["allowed_state_ids"], json!([]));
    let (stored_type, stored_state) = issue_type_state(&st, cross_id).await;
    assert_eq!(stored_type, Some(a.type_id));
    assert_eq!(stored_state, Some(a.state("New")));

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn patch_type_clear_falls_back_to_legacy() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wftnull").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let type_a = insert_issue_type(&st, ws_id, "Incident", false).await;
    let state_a = insert_typed_state(
        &st, project_id, ws_id, type_a, "New", "new", 10000.0, "backlog", true,
    )
    .await;
    let issue_id = create_issue_id(
        &st,
        owner,
        &slug,
        project_id,
        "typed",
        Some(type_a),
        Some(state_a),
    )
    .await;

    let legacy = resolve_issue_state(&st.pool, project_id, None, None)
        .await
        .expect("legacy default");
    assert!(legacy.is_some(), "seed workspace punya default legacy");

    let (status, _) = patch_issue_req(
        &st,
        owner,
        &slug,
        project_id,
        issue_id,
        patch_body(Some(None), None),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (stored_type, stored_state) = issue_type_state(&st, issue_id).await;
    assert_eq!(stored_type, None, "type_id: null harus terhapus");
    assert_eq!(
        stored_state, legacy,
        "fallback ke default legacy tanpa cek transisi"
    );

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn patch_untyped_legacy_state_stays_free() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfuntyped").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let mut legacy = Vec::new();
    for (name, slug_part, sequence) in [
        ("Legacy One", "legacy-one", 70001.0),
        ("Legacy Two", "legacy-two", 70002.0),
    ] {
        let (state_id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO states (id, name, description, color, slug, sequence, \"group\", is_triage, \
             \"default\", project_id, workspace_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, '', '#60646C', $2, $3, 'backlog', false, false, \
             $4, $5, now(), now()) RETURNING id",
        )
        .bind(name)
        .bind(slug_part)
        .bind(sequence)
        .bind(project_id)
        .bind(ws_id)
        .fetch_one(&st.pool)
        .await
        .expect("legacy state");
        legacy.push(state_id);
    }

    let issue_id = create_issue_id(
        &st,
        owner,
        &slug,
        project_id,
        "legacy",
        None,
        Some(legacy[0]),
    )
    .await;
    let (status, _) = patch_issue_req(
        &st,
        owner,
        &slug,
        project_id,
        issue_id,
        patch_body(None, Some(Some(legacy[1]))),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (stored_type, stored_state) = issue_type_state(&st, issue_id).await;
    assert_eq!(stored_type, None);
    assert_eq!(stored_state, Some(legacy[1]));

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn patch_epic_type_state_stays_free() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfepic").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let epic_type = insert_issue_type(&st, ws_id, "Epic", true).await;
    let first = insert_typed_state(
        &st, project_id, ws_id, epic_type, "New", "epic-new", 10000.0, "backlog", true,
    )
    .await;
    let second = insert_typed_state(
        &st,
        project_id,
        ws_id,
        epic_type,
        "In Progress",
        "epic-progress",
        20000.0,
        "started",
        false,
    )
    .await;

    let issue_id = create_issue_id(
        &st,
        owner,
        &slug,
        project_id,
        "epic",
        Some(epic_type),
        Some(first),
    )
    .await;
    let (status, _) = patch_issue_req(
        &st,
        owner,
        &slug,
        project_id,
        issue_id,
        patch_body(None, Some(Some(second))),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "epic tidak di-enforce workflow"
    );
    let (stored_type, stored_state) = issue_type_state(&st, issue_id).await;
    assert_eq!(stored_type, Some(epic_type));
    assert_eq!(stored_state, Some(second));

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn patch_explicit_same_state_is_noop() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfsame").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let type_a = insert_issue_type(&st, ws_id, "Incident", false).await;
    let state_a = insert_typed_state(
        &st, project_id, ws_id, type_a, "New", "new", 10000.0, "backlog", true,
    )
    .await;
    let issue_id = create_issue_id(
        &st,
        owner,
        &slug,
        project_id,
        "same",
        Some(type_a),
        Some(state_a),
    )
    .await;

    let before = state_activity_count(&st, issue_id).await;
    let (status, _) = patch_issue_req(
        &st,
        owner,
        &slug,
        project_id,
        issue_id,
        patch_body(None, Some(Some(state_a))),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(state_activity_count(&st, issue_id).await, before);
    let (_, stored_state) = issue_type_state(&st, issue_id).await;
    assert_eq!(stored_state, Some(state_a));

    purge(&st.pool, &slug).await;
}

async fn make_draft(
    st: &AppState,
    owner: Uuid,
    slug: &str,
    project_id: Uuid,
    name: &str,
    type_id: Option<Uuid>,
    state_id: Option<Uuid>,
) -> Uuid {
    let (status, Json(draft)) = draft_create(
        State(st.clone()),
        AuthUser(owner),
        Path(slug.to_string()),
        Some(Json(CreateDraftBody {
            name: Some(name.to_string()),
            project_id: Some(project_id),
            type_id,
            state_id,
            ..Default::default()
        })),
    )
    .await
    .expect("draft create");
    assert_eq!(status, StatusCode::CREATED, "draft {name}");
    Uuid::parse_str(draft["id"].as_str().expect("draft id")).expect("uuid")
}

async fn convert_draft(
    st: &AppState,
    owner: Uuid,
    slug: &str,
    draft_id: Uuid,
    body: ConvertBody,
) -> (StatusCode, Value) {
    let (status, Json(payload)) = create_draft_to_issue(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.to_string(), draft_id)),
        Some(Json(body)),
    )
    .await
    .expect("convert");
    (status, payload)
}

#[tokio::test]
async fn draft_convert_reconciles_type_and_enforces_transitions() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfdraft").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    // Type X tanpa transisi: perpindahan state apa pun ditolak. Type Y hanya
    // menyediakan default Todo.
    let x = make_workflow_type(
        &st,
        &slug,
        ws_id,
        owner,
        project_id,
        "Incident",
        &[("New", "backlog"), ("In Progress", "started")],
        &[],
    )
    .await;
    let y = make_workflow_type(
        &st,
        &slug,
        ws_id,
        owner,
        project_id,
        "Problem",
        &[("Todo", "backlog"), ("Done", "completed")],
        &[],
    )
    .await;

    // Draft bertipe X + body tanpa type → issue type X + default X.
    let draft_x = make_draft(
        &st,
        owner,
        &slug,
        project_id,
        "draft x",
        Some(x.type_id),
        Some(x.state("New")),
    )
    .await;
    let (status, issue) = convert_draft(
        &st,
        owner,
        &slug,
        draft_x,
        ConvertBody {
            name: Some("issue x".into()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let issue_x = Uuid::parse_str(issue["id"].as_str().expect("issue id")).expect("uuid");
    let (stored_type, stored_state) = issue_type_state(&st, issue_x).await;
    assert_eq!(stored_type, Some(x.type_id));
    assert_eq!(stored_state, Some(x.state("New")));

    // Body type Y ≠ draft X → issue type Y + default Y (body menang).
    let draft_y = make_draft(
        &st,
        owner,
        &slug,
        project_id,
        "draft y",
        Some(x.type_id),
        Some(x.state("New")),
    )
    .await;
    let (status, issue) = convert_draft(
        &st,
        owner,
        &slug,
        draft_y,
        ConvertBody {
            name: Some("issue y".into()),
            type_id: Some(y.type_id),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let issue_y = Uuid::parse_str(issue["id"].as_str().expect("issue id")).expect("uuid");
    let (stored_type, stored_state) = issue_type_state(&st, issue_y).await;
    assert_eq!(stored_type, Some(y.type_id));
    assert_eq!(stored_state, Some(y.state("Todo")));

    // State eksplisit milik type lain → 400, draft tetap hidup.
    let draft_bad = make_draft(
        &st,
        owner,
        &slug,
        project_id,
        "draft bad",
        Some(x.type_id),
        Some(x.state("New")),
    )
    .await;
    let (status, body) = convert_draft(
        &st,
        owner,
        &slug,
        draft_bad,
        ConvertBody {
            name: Some("issue bad".into()),
            state_id: Some(y.state("Todo")),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "State is not valid for this work item type");
    let (alive,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM draft_issues WHERE id = $1 AND deleted_at IS NULL")
            .bind(draft_bad)
            .fetch_one(&st.pool)
            .await
            .expect("draft alive");
    assert_eq!(alive, 1, "convert gagal tidak boleh menghapus draft");

    // Type sama + state pindah tanpa transisi terdaftar → 400.
    let draft_move = make_draft(
        &st,
        owner,
        &slug,
        project_id,
        "draft move",
        Some(x.type_id),
        Some(x.state("New")),
    )
    .await;
    let (status, body) = convert_draft(
        &st,
        owner,
        &slug,
        draft_move,
        ConvertBody {
            name: Some("issue move".into()),
            state_id: Some(x.state("In Progress")),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid state transition");
    assert_eq!(body["allowed_state_ids"], json!([]));
    let (alive,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM draft_issues WHERE id = $1 AND deleted_at IS NULL")
            .bind(draft_move)
            .fetch_one(&st.pool)
            .await
            .expect("draft alive");
    assert_eq!(alive, 1, "convert gagal tidak boleh menghapus draft");

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn draft_convert_type_and_state_guards() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfdraftguard").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let x = make_workflow_type(
        &st,
        &slug,
        ws_id,
        owner,
        project_id,
        "Incident",
        &[("New", "backlog"), ("In Progress", "started")],
        &[],
    )
    .await;
    // Type hidup di workspace yang sama tapi belum enabled di project.
    let not_enabled = insert_issue_type(&st, ws_id, "Not Enabled", false).await;
    let legacy = insert_legacy_state(
        &st,
        project_id,
        ws_id,
        "Legacy Backlog",
        "legacy-backlog-guard",
        51000.0,
        "backlog",
        false,
    )
    .await;
    let legacy_two = insert_legacy_state(
        &st,
        project_id,
        ws_id,
        "Legacy Todo",
        "legacy-todo-guard",
        51001.0,
        "backlog",
        false,
    )
    .await;

    // (1) Type asing / belum enabled di project di body → 400 `type_id is not
    // valid` (bukan FK-violation 500), draft tetap hidup.
    for bad_type in [Uuid::new_v4(), not_enabled] {
        let draft = make_draft(
            &st,
            owner,
            &slug,
            project_id,
            "bad type draft",
            Some(x.type_id),
            Some(x.state("New")),
        )
        .await;
        let (status, body) = convert_draft(
            &st,
            owner,
            &slug,
            draft,
            ConvertBody {
                name: Some("bad type issue".into()),
                type_id: Some(bad_type),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "type {bad_type}");
        assert_eq!(body["error"], "type_id is not valid");
        let (alive,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM draft_issues WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(draft)
        .fetch_one(&st.pool)
        .await
        .expect("draft alive");
        assert_eq!(alive, 1, "convert gagal tidak boleh menghapus draft");
    }

    // (2) Body tanpa state → state draft dipertahankan apa adanya, bukan
    // dipindah paksa ke default (yang bisa 400 tanpa transisi).
    let draft_keep = make_draft(
        &st,
        owner,
        &slug,
        project_id,
        "keep draft",
        Some(x.type_id),
        Some(x.state("In Progress")),
    )
    .await;
    let (status, issue) = convert_draft(
        &st,
        owner,
        &slug,
        draft_keep,
        ConvertBody {
            name: Some("keep issue".into()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let keep_id = Uuid::parse_str(issue["id"].as_str().expect("issue id")).expect("uuid");
    let (stored_type, stored_state) = issue_type_state(&st, keep_id).await;
    assert_eq!(stored_type, Some(x.type_id));
    assert_eq!(
        stored_state,
        Some(x.state("In Progress")),
        "state draft harus dipertahankan"
    );

    // (3) Draft typed + state legacy eksplisit → sukses (state untyped sah).
    let draft_legacy = make_draft(
        &st,
        owner,
        &slug,
        project_id,
        "legacy draft",
        Some(x.type_id),
        Some(legacy),
    )
    .await;
    let (status, issue) = convert_draft(
        &st,
        owner,
        &slug,
        draft_legacy,
        ConvertBody {
            name: Some("legacy issue".into()),
            state_id: Some(legacy),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let legacy_id = Uuid::parse_str(issue["id"].as_str().expect("issue id")).expect("uuid");
    let (stored_type, stored_state) = issue_type_state(&st, legacy_id).await;
    assert_eq!(stored_type, Some(x.type_id));
    assert_eq!(stored_state, Some(legacy));

    // (4) Draft tanpa state + explicit typed non-default → cek transisi awal:
    // hanya default type yang diizinkan.
    let draft_null = make_draft(
        &st,
        owner,
        &slug,
        project_id,
        "null state draft",
        Some(x.type_id),
        Some(x.state("New")),
    )
    .await;
    sqlx::query("UPDATE draft_issues SET state_id = NULL WHERE id = $1")
        .bind(draft_null)
        .execute(&st.pool)
        .await
        .expect("null draft state");
    let (status, body) = convert_draft(
        &st,
        owner,
        &slug,
        draft_null,
        ConvertBody {
            name: Some("null state issue".into()),
            state_id: Some(x.state("In Progress")),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid state transition");
    assert_eq!(
        body["allowed_state_ids"],
        json!([x.state("New").to_string()])
    );

    // (5) Draft untyped: pindah state legacy bebas (tanpa enforcement).
    let draft_untyped = make_draft(
        &st,
        owner,
        &slug,
        project_id,
        "untyped draft",
        None,
        Some(legacy),
    )
    .await;
    let (status, issue) = convert_draft(
        &st,
        owner,
        &slug,
        draft_untyped,
        ConvertBody {
            name: Some("untyped issue".into()),
            state_id: Some(legacy_two),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let untyped_id = Uuid::parse_str(issue["id"].as_str().expect("issue id")).expect("uuid");
    let (stored_type, stored_state) = issue_type_state(&st, untyped_id).await;
    assert_eq!(stored_type, None);
    assert_eq!(stored_state, Some(legacy_two));

    // (6) Draft epic.
    let epic_type = insert_issue_type(&st, ws_id, "Epic Draft", true).await;
    let epic_first = insert_typed_state(
        &st,
        project_id,
        ws_id,
        epic_type,
        "New",
        "epic-draft-new",
        52000.0,
        "backlog",
        true,
    )
    .await;
    let epic_second = insert_typed_state(
        &st,
        project_id,
        ws_id,
        epic_type,
        "In Progress",
        "epic-draft-progress",
        52001.0,
        "started",
        false,
    )
    .await;

    // (6a) Body meng-echo type epic draft sendiri → diterima (FE menyebar
    // payload draft), state draft dipertahankan.
    let draft_epic_echo = make_draft(
        &st,
        owner,
        &slug,
        project_id,
        "epic echo draft",
        Some(epic_type),
        Some(epic_first),
    )
    .await;
    let (status, issue) = convert_draft(
        &st,
        owner,
        &slug,
        draft_epic_echo,
        ConvertBody {
            name: Some("epic echo issue".into()),
            type_id: Some(epic_type),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "epic echo type draft sendiri");
    let epic_echo_id = Uuid::parse_str(issue["id"].as_str().expect("issue id")).expect("uuid");
    let (stored_type, stored_state) = issue_type_state(&st, epic_echo_id).await;
    assert_eq!(stored_type, Some(epic_type));
    assert_eq!(stored_state, Some(epic_first));

    // (6b) Epic LAIN di body → 400 `type_id is not valid`.
    let epic_other = insert_issue_type(&st, ws_id, "Epic Other", true).await;
    let draft_epic_other = make_draft(
        &st,
        owner,
        &slug,
        project_id,
        "epic other draft",
        Some(epic_type),
        Some(epic_first),
    )
    .await;
    let (status, body) = convert_draft(
        &st,
        owner,
        &slug,
        draft_epic_other,
        ConvertBody {
            name: Some("epic other issue".into()),
            type_id: Some(epic_other),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "type_id is not valid");

    // (6c) Body tanpa type: enforcement dilewati, state eksplisit bebas.
    let draft_epic = make_draft(
        &st,
        owner,
        &slug,
        project_id,
        "epic draft",
        Some(epic_type),
        Some(epic_first),
    )
    .await;
    let (status, issue) = convert_draft(
        &st,
        owner,
        &slug,
        draft_epic,
        ConvertBody {
            name: Some("epic issue".into()),
            state_id: Some(epic_second),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "epic draft convert bebas");
    let epic_id = Uuid::parse_str(issue["id"].as_str().expect("issue id")).expect("uuid");
    let (stored_type, stored_state) = issue_type_state(&st, epic_id).await;
    assert_eq!(stored_type, Some(epic_type));
    assert_eq!(stored_state, Some(epic_second));

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn draft_convert_drops_kept_state_of_other_type() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfdraftkept").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let x = make_workflow_type(
        &st,
        &slug,
        ws_id,
        owner,
        project_id,
        "Incident",
        &[("New", "backlog"), ("In Progress", "started")],
        &[],
    )
    .await;
    let y = make_workflow_type(
        &st,
        &slug,
        ws_id,
        owner,
        project_id,
        "Problem",
        &[("Todo", "backlog")],
        &[],
    )
    .await;

    // Regresi review: draft type X + state milik type Y; body TANPA state.
    // State draft tidak boleh ikut (mismatch type) — harus jatuh ke default X.
    let draft = make_draft(
        &st,
        owner,
        &slug,
        project_id,
        "mismatch draft",
        Some(x.type_id),
        Some(y.state("Todo")),
    )
    .await;
    let (status, issue) = convert_draft(
        &st,
        owner,
        &slug,
        draft,
        ConvertBody {
            name: Some("mismatch issue".into()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let issue_id = Uuid::parse_str(issue["id"].as_str().expect("issue id")).expect("uuid");
    let (stored_type, stored_state) = issue_type_state(&st, issue_id).await;
    assert_eq!(stored_type, Some(x.type_id));
    assert_eq!(
        stored_state,
        Some(x.state("New")),
        "kept state type Y harus diganti default type X"
    );
    assert_ne!(stored_state, Some(y.state("Todo")));

    purge(&st.pool, &slug).await;
}

// --- DB-backed v1 public API write enforcement (C5) ------------------------

async fn v1_update_issue(
    st: &AppState,
    owner: Uuid,
    slug: &str,
    project_id: Uuid,
    issue_id: Uuid,
    body: V1WriteWorkItem,
) -> (StatusCode, Value) {
    let (status, Json(payload)) = v1_update(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.to_string(), project_id, issue_id)),
        Json(body),
    )
    .await
    .expect("v1 update");
    (status, payload)
}

#[tokio::test]
async fn v1_update_enforces_transitions() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfv1upd").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let a = make_workflow_type(
        &st,
        &slug,
        ws_id,
        owner,
        project_id,
        "Incident",
        &[
            ("New", "backlog"),
            ("In Progress", "started"),
            ("Closed", "completed"),
        ],
        &[("New", "In Progress")],
    )
    .await;
    let issue_id = create_issue_id(
        &st,
        owner,
        &slug,
        project_id,
        "v1 issue",
        Some(a.type_id),
        Some(a.state("New")),
    )
    .await;

    // New → Closed tidak terdaftar → 400 + allowed persis In Progress.
    let (status, body) = v1_update_issue(
        &st,
        owner,
        &slug,
        project_id,
        issue_id,
        V1WriteWorkItem {
            state: Some(a.state("Closed")),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid state transition");
    assert_eq!(
        body["allowed_state_ids"],
        json!([a.state("In Progress").to_string()])
    );
    let (stored_type, stored_state) = issue_type_state(&st, issue_id).await;
    assert_eq!(stored_type, Some(a.type_id));
    assert_eq!(
        stored_state,
        Some(a.state("New")),
        "update yang ditolak tidak boleh menyentuh state"
    );

    // New → In Progress terdaftar → 200 dan tersimpan.
    let (status, _) = v1_update_issue(
        &st,
        owner,
        &slug,
        project_id,
        issue_id,
        V1WriteWorkItem {
            state: Some(a.state("In Progress")),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (stored_type, stored_state) = issue_type_state(&st, issue_id).await;
    assert_eq!(stored_type, Some(a.type_id));
    assert_eq!(stored_state, Some(a.state("In Progress")));

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn v1_update_type_change_reconciles_state() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfv1tc").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let a = make_workflow_type(
        &st,
        &slug,
        ws_id,
        owner,
        project_id,
        "Incident",
        &[
            ("New", "backlog"),
            ("In Progress", "started"),
            ("Closed", "completed"),
        ],
        &[("New", "In Progress")],
    )
    .await;
    let b = make_workflow_type(
        &st,
        &slug,
        ws_id,
        owner,
        project_id,
        "Service Request",
        &[("Todo", "backlog"), ("Done", "completed")],
        &[("Todo", "Done")],
    )
    .await;

    // Ganti type + state milik type LAMA → 400, row tidak berubah.
    let issue_id = create_issue_id(
        &st,
        owner,
        &slug,
        project_id,
        "typed one",
        Some(a.type_id),
        Some(a.state("New")),
    )
    .await;
    let (status, body) = v1_update_issue(
        &st,
        owner,
        &slug,
        project_id,
        issue_id,
        V1WriteWorkItem {
            type_id: Some(b.type_id),
            state: Some(a.state("New")),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "State is not valid for this work item type");
    let (stored_type, stored_state) = issue_type_state(&st, issue_id).await;
    assert_eq!(stored_type, Some(a.type_id));
    assert_eq!(stored_state, Some(a.state("New")));

    // Ganti type + state valid milik type BARU: tanpa cek transisi (B.Done
    // tidak terjangkau dari workflow A).
    let (status, _) = v1_update_issue(
        &st,
        owner,
        &slug,
        project_id,
        issue_id,
        V1WriteWorkItem {
            type_id: Some(b.type_id),
            state: Some(b.state("Done")),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (stored_type, stored_state) = issue_type_state(&st, issue_id).await;
    assert_eq!(stored_type, Some(b.type_id));
    assert_eq!(stored_state, Some(b.state("Done")));

    // Ganti type + state legacy untyped eksplisit → 400: type change
    // mensyaratkan state milik type BARU secara ketat (tanpa allowance legacy).
    let legacy = insert_legacy_state(
        &st,
        project_id,
        ws_id,
        "Legacy Backlog",
        "legacy-backlog-tc",
        53000.0,
        "backlog",
        false,
    )
    .await;
    let legacy_issue = create_issue_id(
        &st,
        owner,
        &slug,
        project_id,
        "typed legacy",
        Some(a.type_id),
        Some(a.state("New")),
    )
    .await;
    let (status, body) = v1_update_issue(
        &st,
        owner,
        &slug,
        project_id,
        legacy_issue,
        V1WriteWorkItem {
            type_id: Some(b.type_id),
            state: Some(legacy),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "State is not valid for this work item type");
    let (stored_type, stored_state) = issue_type_state(&st, legacy_issue).await;
    assert_eq!(stored_type, Some(a.type_id));
    assert_eq!(stored_state, Some(a.state("New")));

    // Ganti type tanpa state → default type baru.
    let moved = create_issue_id(
        &st,
        owner,
        &slug,
        project_id,
        "typed two",
        Some(a.type_id),
        Some(a.state("New")),
    )
    .await;
    let (status, _) = v1_update_issue(
        &st,
        owner,
        &slug,
        project_id,
        moved,
        V1WriteWorkItem {
            type_id: Some(b.type_id),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (stored_type, stored_state) = issue_type_state(&st, moved).await;
    assert_eq!(stored_type, Some(b.type_id));
    assert_eq!(stored_state, Some(b.state("Todo")));

    purge(&st.pool, &slug).await;
}

#[tokio::test]
async fn v1_create_type_aware_default_and_state() {
    let st = app_state().await;
    let (slug, ws_id, project_id) = make_workspace(&st, "wfv1create").await;
    let (owner,): (Uuid,) = sqlx::query_as("SELECT owner_id FROM workspaces WHERE slug = $1")
        .bind(&slug)
        .fetch_one(&st.pool)
        .await
        .expect("owner");

    let type_a = insert_issue_type(&st, ws_id, "Incident", false).await;
    let state_a = insert_typed_state(
        &st, project_id, ws_id, type_a, "New", "new", 10000.0, "backlog", true,
    )
    .await;
    let type_b = insert_issue_type(&st, ws_id, "Problem", false).await;
    let state_b = insert_typed_state(
        &st, project_id, ws_id, type_b, "TriageB", "triage-b", 10000.0, "backlog", true,
    )
    .await;
    for t in [type_a, type_b] {
        let (status, _) = import_to_project(
            State(st.clone()),
            AuthUser(owner),
            Path((slug.clone(), project_id)),
            Json(json!({"work_item_types": [t]})),
        )
        .await
        .expect("import");
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    // State milik type B dikirim dengan type A → 400 type message.
    let (status, body) = v1_create(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
        Json(V1WriteWorkItem {
            name: Some("Server down".into()),
            state: Some(state_b),
            type_id: Some(type_a),
            ..Default::default()
        }),
    )
    .await
    .expect("v1 create cross-type state");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "State is not valid for this work item type");

    // Type tanpa state → default milik type (bukan default legacy).
    let (status, Json(issue)) = v1_create(
        State(st.clone()),
        AuthUser(owner),
        Path((slug.clone(), project_id)),
        Json(V1WriteWorkItem {
            name: Some("Server up".into()),
            type_id: Some(type_a),
            ..Default::default()
        }),
    )
    .await
    .expect("v1 create typed");
    assert_eq!(status, StatusCode::CREATED);
    let issue_id = Uuid::parse_str(issue["id"].as_str().expect("issue id")).expect("uuid");
    let (stored_type, stored_state) = issue_type_state(&st, issue_id).await;
    assert_eq!(stored_type, Some(type_a));
    assert_eq!(stored_state, Some(state_a));

    purge(&st.pool, &slug).await;
}
