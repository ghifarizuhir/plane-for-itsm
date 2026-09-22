//! Regression tests for the legacy issue PATCH handler
//! (`issue_update::patch_issue`): request validation and handler contract
//! (later tasks extend the write assertions).

use api::middleware::auth::AuthUser;
use api::routes::issue_update::{patch_issue, PatchIssue};
use api::routes::issue_write::{create, CreateIssue};
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

async fn state() -> AppState {
    AppState {
        pool: pool().await,
        redis: redis::Client::open("redis://127.0.0.1:6379").expect("redis client"),
        config: AppConfig::from_env(),
    }
}

struct Scratch {
    slug: String,
    workspace_id: Uuid,
    user_id: Uuid,
    project_id: Uuid,
    state_id: Uuid,
    extra_users: Vec<Uuid>,
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
         VALUES (gen_random_uuid(), now(), now(), $1, $2, $3, '{}', '{}', '{}', '{}', '{}', \
         '{}', true)",
    )
    .bind(role)
    .bind(user_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch workspace member");
}

async fn insert_project_member(
    pool: &PgPool,
    user_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    role: i16,
) {
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

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        purge(pool).await;

        let slug = format!("itpat-{}", Uuid::new_v4().simple());
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let state_id = Uuid::new_v4();
        let identifier =
            format!("ITSQ{}", &Uuid::new_v4().simple().to_string()[..8]).to_uppercase();

        insert_user(pool, user_id, &slug).await;

        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'IT Seq Scratch', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
        )
        .bind(workspace_id)
        .bind(&slug)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("scratch workspace");

        insert_workspace_member(pool, user_id, workspace_id, 20).await;

        sqlx::query(
            "INSERT INTO projects (id, created_at, updated_at, name, description, network, \
             identifier, workspace_id, cycle_view, module_view, issue_views_view, page_view, \
             intake_view, archive_in, close_in, logo_props, is_time_tracking_enabled, \
             is_issue_type_enabled, guest_view_all_features, timezone) \
             VALUES ($1, now(), now(), 'IT Seq Scratch', '', 2, $2, $3, false, false, false, \
             false, false, 30, 30, '{}'::jsonb, false, false, false, 'UTC')",
        )
        .bind(project_id)
        .bind(&identifier)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch project");

        insert_project_member(pool, user_id, project_id, workspace_id, 20).await;

        sqlx::query(
            "INSERT INTO states (id, name, description, color, slug, project_id, workspace_id, \
             sequence, \"group\", \"default\", is_triage, created_at, updated_at) \
             VALUES ($1, 'Backlog', '', '#60646C', 'backlog', $2, $3, 65535, 'backlog', true, \
             false, now(), now())",
        )
        .bind(state_id)
        .bind(project_id)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch state");

        Self {
            slug,
            workspace_id,
            user_id,
            project_id,
            state_id,
            extra_users: Vec::new(),
        }
    }

    async fn add_actor(
        &mut self,
        pool: &PgPool,
        ws_role: Option<i16>,
        project_role: Option<i16>,
    ) -> Uuid {
        let user_id = Uuid::new_v4();
        let username = format!("{}-{}", self.slug, &user_id.simple().to_string()[..8]);
        insert_user(pool, user_id, &username).await;
        if let Some(role) = ws_role {
            insert_workspace_member(pool, user_id, self.workspace_id, role).await;
        }
        if let Some(role) = project_role {
            insert_project_member(pool, user_id, self.project_id, self.workspace_id, role).await;
        }
        self.extra_users.push(user_id);
        user_id
    }

    async fn cleanup(&self, pool: &PgPool) {
        sqlx::query("DELETE FROM intake_issues WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM issue_sequences WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM estimate_points WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM estimates WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM issue_types WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM issue_assignees WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM issue_labels WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM labels WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM issue_activities WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM issue_subscribers WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM issue_description_versions WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM issues WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM intakes WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM states WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM project_members WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM projects WHERE id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(self.user_id)
            .execute(pool)
            .await
            .ok();
        for user_id in &self.extra_users {
            sqlx::query("DELETE FROM users WHERE id = $1")
                .bind(user_id)
                .execute(pool)
                .await
                .ok();
        }
    }
}

async fn insert_label(pool: &PgPool, project_id: Uuid, workspace_id: Uuid, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO labels (id, name, color, description, sort_order, project_id, workspace_id, \
         created_at, updated_at) \
         VALUES ($1, $2, '#60646C', '', 65535, $3, $4, now(), now())",
    )
    .bind(id)
    .bind(name)
    .bind(project_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch label");
    id
}

async fn insert_estimate_with_point(pool: &PgPool, project_id: Uuid, workspace_id: Uuid) -> Uuid {
    let estimate_id = Uuid::new_v4();
    let point_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO estimates (id, name, description, type, last_used, project_id, workspace_id, \
         created_at, updated_at) \
         VALUES ($1, 'Points', '', 'points', true, $2, $3, now(), now())",
    )
    .bind(estimate_id)
    .bind(project_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch estimate");
    sqlx::query(
        "INSERT INTO estimate_points (id, estimate_id, key, value, description, project_id, \
         workspace_id, created_at, updated_at) \
         VALUES ($1, $2, 0, '1', '', $3, $4, now(), now())",
    )
    .bind(point_id)
    .bind(estimate_id)
    .bind(project_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch estimate point");
    point_id
}

async fn insert_state(
    pool: &PgPool,
    project_id: Uuid,
    workspace_id: Uuid,
    name: &str,
    group: &str,
    is_default: bool,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO states (id, name, description, color, slug, project_id, workspace_id, sequence, \
         \"group\", \"default\", is_triage, created_at, updated_at) \
         VALUES ($1, $2, '', '#60646C', $3, $4, $5, 65535, $6, $7, false, now(), now())",
    )
    .bind(id)
    .bind(name)
    .bind(name.to_lowercase())
    .bind(project_id)
    .bind(workspace_id)
    .bind(group)
    .bind(is_default)
    .execute(pool)
    .await
    .expect("scratch state");
    id
}

#[derive(Debug, sqlx::FromRow)]
struct IssueRow {
    name: String,
    description_html: String,
    description_stripped: Option<String>,
    description_json: Value,
    priority: String,
    state_id: Option<Uuid>,
    parent_id: Option<Uuid>,
    start_date: Option<chrono::NaiveDate>,
    target_date: Option<chrono::NaiveDate>,
    sort_order: f64,
    point: Option<i32>,
    estimate_point_id: Option<Uuid>,
    type_id: Option<Uuid>,
    updated_by_id: Option<Uuid>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

async fn issue_row(pool: &PgPool, issue_id: Uuid) -> IssueRow {
    sqlx::query_as(
        "SELECT name, description_html, description_stripped, description_json, priority, state_id, \
         parent_id, start_date, target_date, sort_order, point, estimate_point_id, type_id, \
         updated_by_id, completed_at FROM issues WHERE id = $1",
    )
    .bind(issue_id)
    .fetch_one(pool)
    .await
    .expect("issue row")
}

async fn live_assignees(pool: &PgPool, issue_id: Uuid) -> Vec<Uuid> {
    sqlx::query_scalar(
        "SELECT assignee_id FROM issue_assignees WHERE issue_id = $1 AND deleted_at IS NULL \
         ORDER BY created_at",
    )
    .bind(issue_id)
    .fetch_all(pool)
    .await
    .expect("assignee rows")
}

async fn live_labels(pool: &PgPool, issue_id: Uuid) -> Vec<Uuid> {
    sqlx::query_scalar(
        "SELECT label_id FROM issue_labels WHERE issue_id = $1 AND deleted_at IS NULL ORDER BY created_at",
    )
    .bind(issue_id)
    .fetch_all(pool)
    .await
    .expect("label rows")
}

async fn insert_issue_type(pool: &PgPool, workspace_id: Uuid, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issue_types (id, name, description, logo_props, workspace_id, is_active, \
         is_default, level, is_epic, created_at, updated_at) \
         VALUES ($1, $2, '', '{}'::jsonb, $3, true, false, 0, false, now(), now())",
    )
    .bind(id)
    .bind(name)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch issue type");
    id
}

/// Drop leftovers from earlier failed runs so scratch slugs never collide.
async fn purge(pool: &PgPool) {
    let stale: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM workspaces WHERE slug LIKE 'itpat-%'")
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    for workspace_id in stale {
        let projects: Vec<Uuid> =
            sqlx::query_scalar("SELECT id FROM projects WHERE workspace_id = $1")
                .bind(workspace_id)
                .fetch_all(pool)
                .await
                .unwrap_or_default();
        for project_id in &projects {
            for stmt in [
                "DELETE FROM intake_issues WHERE project_id = $1",
                "DELETE FROM issue_sequences WHERE project_id = $1",
                "DELETE FROM issue_assignees WHERE project_id = $1",
                "DELETE FROM issue_labels WHERE project_id = $1",
                "DELETE FROM issue_activities WHERE project_id = $1",
                "DELETE FROM issue_subscribers WHERE project_id = $1",
                "DELETE FROM issue_description_versions WHERE project_id = $1",
                "DELETE FROM labels WHERE project_id = $1",
                "DELETE FROM issues WHERE project_id = $1",
                "DELETE FROM intakes WHERE project_id = $1",
                "DELETE FROM states WHERE project_id = $1",
                "DELETE FROM project_members WHERE project_id = $1",
                "DELETE FROM estimate_points WHERE project_id = $1",
                "DELETE FROM estimates WHERE project_id = $1",
            ] {
                let _ = sqlx::query(stmt).bind(project_id).execute(pool).await;
            }
        }
        let _ = sqlx::query("DELETE FROM issue_types WHERE workspace_id = $1")
            .bind(workspace_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM projects WHERE workspace_id = $1")
            .bind(workspace_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1")
            .bind(workspace_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(workspace_id)
            .execute(pool)
            .await;
    }
    let _ = sqlx::query("DELETE FROM users WHERE username LIKE 'itpat-%'")
        .execute(pool)
        .await;
}

fn base_body(name: &str, state_id: Uuid) -> CreateIssue {
    CreateIssue {
        name: name.to_string(),
        assignee_ids: None,
        label_ids: None,
        state_id: Some(state_id),
        description_html: None,
        priority: None,
        start_date: None,
        target_date: None,
        parent_id: None,
        type_id: None,
        estimate_point: None,
    }
}

async fn create_issue(st: &AppState, scratch: &Scratch, name: &str) -> Uuid {
    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(base_body(name, scratch.state_id)),
    )
    .await
    .expect("create must return a response");
    assert_eq!(status, StatusCode::CREATED);
    body["id"].as_str().expect("id").parse().expect("uuid")
}

fn patch(body: Value) -> PatchIssue {
    serde_json::from_value(body).expect("patch body must deserialize")
}

async fn patch_issue_req(
    st: &AppState,
    scratch: &Scratch,
    actor: Uuid,
    issue_id: Uuid,
    body: PatchIssue,
) -> (StatusCode, Value) {
    let (status, Json(body)) = patch_issue(
        State(st.clone()),
        AuthUser(actor),
        Path((scratch.slug.clone(), scratch.project_id, issue_id)),
        Json(body),
    )
    .await
    .expect("PATCH must return a response");
    (status, body)
}

#[tokio::test]
async fn patch_403_for_non_member_and_outsider() {
    let st = state().await;
    let pool = pool().await;
    let mut scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "authz").await;
    let outsider = scratch.add_actor(&pool, None, None).await;
    let ws_only = scratch.add_actor(&pool, Some(15), None).await;

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        outsider,
        issue_id,
        patch(json!({"name": "x"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        ws_only,
        issue_id,
        patch(json!({"name": "x"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_denied_miss_is_403_not_404() {
    // Django `@allow_permission` runs before the body/queryset: a denied
    // caller gets 403 even when the issue does not exist (`base.py:627`).
    let st = state().await;
    let pool = pool().await;
    let mut scratch = Scratch::new(&pool).await;
    let outsider = scratch.add_actor(&pool, None, None).await;
    let ws_only = scratch.add_actor(&pool, Some(15), None).await;
    let missing = Uuid::new_v4();

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        outsider,
        missing,
        patch(json!({"name": "x"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) =
        patch_issue_req(&st, &scratch, ws_only, missing, patch(json!({"name": "x"}))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_404_miss_body_is_verbatim() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;

    let (status, body) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        Uuid::new_v4(),
        patch(json!({"name": "x"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({"error": "Issue not found"}));

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_400_validation_messages() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "validation").await;

    let label = insert_label(&pool, scratch.project_id, scratch.workspace_id, "l1").await;
    let estimate_point =
        insert_estimate_with_point(&pool, scratch.project_id, scratch.workspace_id).await;
    let other_issue_id = create_issue(&st, &scratch, "other").await;

    for (body, expected) in [
        (json!({"name": ""}), "name must not be blank"),
        (json!({"name": null}), "name may not be null"),
        (json!({"name": "a".repeat(256)}), "name max length 255"),
        (json!({"priority": "nope"}), "Invalid priority"),
        (json!({"priority": null}), "priority may not be null"),
        (
            json!({"start_date": "not-a-date"}),
            "Invalid date: not-a-date",
        ),
        (
            json!({"start_date": "2026-09-30", "target_date": "2026-09-01"}),
            "Start date cannot exceed target date",
        ),
        (
            json!({"description_html": null}),
            "description_html may not be null",
        ),
        (json!({"description": null}), "description may not be null"),
        (json!({"sort_order": null}), "sort_order may not be null"),
        (json!({"point": 13}), "point must be between 0 and 12"),
        (
            json!({"assignee_ids": null}),
            "assignee_ids may not be null",
        ),
        (json!({"label_ids": null}), "label_ids may not be null"),
        (
            json!({"assignee_ids": [Uuid::new_v4()]}),
            "invalid assignee: not a project member",
        ),
        (
            json!({"label_ids": [Uuid::new_v4()]}),
            "invalid label: not in project",
        ),
        (
            json!({"state_id": Uuid::new_v4()}),
            "State is not valid please pass a valid state_id",
        ),
        (json!({"parent_id": Uuid::new_v4()}), "parent is not valid"),
        (
            json!({"estimate_point": Uuid::new_v4()}),
            "estimate_point is not valid",
        ),
        (json!({"type_id": Uuid::new_v4()}), "type_id is not valid"),
    ] {
        let (status, body) =
            patch_issue_req(&st, &scratch, scratch.user_id, issue_id, patch(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "body: {body}");
        assert_eq!(body["error"], json!(expected), "body: {body}");
    }

    // A start_date without target_date does NOT cross-check (Django compares
    // only when both keys are in `attrs`).
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"start_date": "2026-09-30"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Valid refs pass validation (validation-only slice: writes come next).
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({
            "state_id": scratch.state_id,
            "label_ids": [label],
            "estimate_point": estimate_point,
            "parent_id": other_issue_id,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_persists_scalars_and_recomputes_stripped() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "scalars").await;
    let label = insert_label(&pool, scratch.project_id, scratch.workspace_id, "l1").await;

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({
            "name": "renamed",
            "description_html": "<p>hello <b>world</b></p>",
            "description": {"type": "doc"},
            "priority": "high",
            "start_date": "2026-09-01",
            "target_date": "2026-09-30",
            "sort_order": 1234.5,
            "point": 3,
            "parent_id": null,
            "label_ids": [label],
            "project_id": Uuid::new_v4(),
            "id": Uuid::new_v4(),
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let row = issue_row(&pool, issue_id).await;
    assert_eq!(row.name, "renamed");
    assert_eq!(row.description_html, "<p>hello <b>world</b></p>");
    assert_eq!(row.description_stripped.as_deref(), Some("hello world"));
    assert_eq!(row.description_json, json!({"type": "doc"}));
    assert_eq!(row.priority, "high");
    assert_eq!(
        row.start_date.map(|d| d.to_string()),
        Some("2026-09-01".to_string())
    );
    assert_eq!(
        row.target_date.map(|d| d.to_string()),
        Some("2026-09-30".to_string())
    );
    assert_eq!(row.sort_order, 1234.5);
    assert_eq!(row.point, Some(3));
    assert_eq!(row.updated_by_id, Some(scratch.user_id));

    // Explicit nulls clear nullable fields; empty html stores NULL stripped.
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({
            "start_date": null,
            "target_date": null,
            "point": null,
            "description_html": "",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let row = issue_row(&pool, issue_id).await;
    assert_eq!(row.start_date, None);
    assert_eq!(row.target_date, None);
    assert_eq!(row.point, None);
    assert_eq!(row.description_html, "");
    assert_eq!(row.description_stripped, None);

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_state_writes_completed_at_and_default_fallback() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "state").await;

    let done = insert_state(
        &pool,
        scratch.project_id,
        scratch.workspace_id,
        "Done",
        "completed",
        false,
    )
    .await;
    let started = insert_state(
        &pool,
        scratch.project_id,
        scratch.workspace_id,
        "Doing",
        "started",
        false,
    )
    .await;

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"state_id": done})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let row = issue_row(&pool, issue_id).await;
    assert_eq!(row.state_id, Some(done));
    assert!(row.completed_at.is_some());

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"state_id": started})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let row = issue_row(&pool, issue_id).await;
    assert_eq!(row.state_id, Some(started));
    assert_eq!(row.completed_at, None);

    // `state_id: null` falls back to the default state (Django
    // `_ensure_default_state`); the fixture default is `scratch.state_id`.
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"state_id": null})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let row = issue_row(&pool, issue_id).await;
    assert_eq!(row.state_id, Some(scratch.state_id));
    assert_eq!(row.completed_at, None);

    // An issue whose state is NULL gets the default on ANY update
    // (`Issue.save` runs `_ensure_default_state` every save).
    sqlx::query("UPDATE issues SET state_id = NULL WHERE id = $1")
        .bind(issue_id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"priority": "low"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let row = issue_row(&pool, issue_id).await;
    assert_eq!(row.state_id, Some(scratch.state_id));

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_type_and_estimate_persist() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "refs").await;
    let estimate_point =
        insert_estimate_with_point(&pool, scratch.project_id, scratch.workspace_id).await;
    let issue_type = insert_issue_type(&pool, scratch.workspace_id, "Bug").await;

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"estimate_point": estimate_point, "type_id": issue_type})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let row = issue_row(&pool, issue_id).await;
    assert_eq!(row.estimate_point_id, Some(estimate_point));
    assert_eq!(row.type_id, Some(issue_type));

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"estimate_point": null, "type_id": null})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let row = issue_row(&pool, issue_id).await;
    assert_eq!(row.estimate_point_id, None);
    assert_eq!(row.type_id, None);

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_rejects_triage_and_deleted_states() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "triage-state").await;

    let triage = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO states (id, name, description, color, slug, project_id, workspace_id, sequence, \
         \"group\", \"default\", is_triage, created_at, updated_at) \
         VALUES ($1, 'Triage', '', '#60646C', 'triage', $2, $3, 65535, 'triage', false, true, now(), now())",
    )
    .bind(triage)
    .bind(scratch.project_id)
    .bind(scratch.workspace_id)
    .execute(&pool)
    .await
    .unwrap();

    let deleted = insert_state(
        &pool,
        scratch.project_id,
        scratch.workspace_id,
        "Gone",
        "backlog",
        false,
    )
    .await;
    sqlx::query("UPDATE states SET deleted_at = now() WHERE id = $1")
        .bind(deleted)
        .execute(&pool)
        .await
        .unwrap();

    for state_id in [triage, deleted] {
        let (status, body) = patch_issue_req(
            &st,
            &scratch,
            scratch.user_id,
            issue_id,
            patch(json!({"state_id": state_id})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "state {state_id}");
        assert_eq!(
            body["error"],
            json!("State is not valid please pass a valid state_id")
        );
    }
    assert_eq!(
        issue_row(&pool, issue_id).await.state_id,
        Some(scratch.state_id),
        "state untouched"
    );

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_parent_set_and_clear() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "child").await;
    let parent_id = create_issue(&st, &scratch, "parent").await;

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"parent_id": parent_id})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(issue_row(&pool, issue_id).await.parent_id, Some(parent_id));

    let expected_new: String = sqlx::query_scalar(
        "SELECT p.identifier || '-' || i.sequence_id FROM issues i JOIN projects p ON p.id = i.project_id WHERE i.id = $1",
    )
    .bind(parent_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let rows = activities(&pool, issue_id).await;
    let parent_row = rows
        .iter()
        .find(|r| r.1.as_deref() == Some("parent"))
        .expect("parent activity");
    assert_eq!(parent_row.2, "updated the parent issue to");
    assert_eq!(parent_row.3.as_deref(), Some(""));
    assert_eq!(parent_row.4.as_deref(), Some(expected_new.as_str()));
    assert_eq!(parent_row.5, None);
    assert_eq!(parent_row.6, Some(parent_id));

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"parent_id": null})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(issue_row(&pool, issue_id).await.parent_id, None);

    let rows = activities(&pool, issue_id).await;
    let clear_row = rows
        .iter()
        .rev()
        .find(|r| r.1.as_deref() == Some("parent") && r.4.as_deref() == Some(""))
        .expect("parent clear activity");
    assert_eq!(clear_row.3.as_deref(), Some(expected_new.as_str()));
    assert_eq!(clear_row.5, Some(parent_id));
    assert_eq!(clear_row.6, None);

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_same_state_does_not_restamp_completed_at() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "no-restamp").await;
    let done = insert_state(
        &pool,
        scratch.project_id,
        scratch.workspace_id,
        "Done",
        "completed",
        false,
    )
    .await;

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"state_id": done})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let first = issue_row(&pool, issue_id)
        .await
        .completed_at
        .expect("completed_at set");

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"state_id": done, "sort_order": 42.0})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let second = issue_row(&pool, issue_id)
        .await
        .completed_at
        .expect("completed_at still set");
    assert_eq!(first, second, "same state must not restamp completed_at");

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_recomputes_stripped_from_stored_html_when_html_absent() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "stripped").await;

    sqlx::query("UPDATE issues SET description_html = '<p>stored</p>', description_stripped = NULL WHERE id = $1")
        .bind(issue_id)
        .execute(&pool)
        .await
        .unwrap();

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"priority": "low"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        issue_row(&pool, issue_id)
            .await
            .description_stripped
            .as_deref(),
        Some("stored")
    );

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_replaces_assignee_and_label_bridges() {
    let st = state().await;
    let pool = pool().await;
    let mut scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "bridges").await;
    let member = scratch.add_actor(&pool, Some(15), Some(15)).await;
    let label_a = insert_label(&pool, scratch.project_id, scratch.workspace_id, "a").await;
    let label_b = insert_label(&pool, scratch.project_id, scratch.workspace_id, "b").await;

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"assignee_ids": [member, member], "label_ids": [label_a]})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(live_assignees(&pool, issue_id).await, vec![member]);
    assert_eq!(live_labels(&pool, issue_id).await, vec![label_a]);

    // Replace: one live row, the previous row is soft-deleted.
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"assignee_ids": [scratch.user_id], "label_ids": [label_b]})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(live_assignees(&pool, issue_id).await, vec![scratch.user_id]);
    assert_eq!(live_labels(&pool, issue_id).await, vec![label_b]);
    let soft: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_assignees WHERE issue_id = $1 AND deleted_at IS NOT NULL",
    )
    .bind(issue_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(soft, 1, "replaced bridge rows are soft-deleted");

    // `[]` clears.
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"assignee_ids": [], "label_ids": []})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(live_assignees(&pool, issue_id).await.is_empty());
    assert!(live_labels(&pool, issue_id).await.is_empty());

    // A change to one bridge must not touch the other. A fresh issue keeps
    // the soft-delete counters clean (the steps above already soft-deleted
    // two label rows on `issue_id`).
    let isolated_id = create_issue(&st, &scratch, "bridge-isolation").await;
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        isolated_id,
        patch(json!({"assignee_ids": [member], "label_ids": [label_a]})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        isolated_id,
        patch(json!({"assignee_ids": []})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(live_assignees(&pool, isolated_id).await.is_empty());
    assert_eq!(
        live_labels(&pool, isolated_id).await,
        vec![label_a],
        "labels untouched by assignee clear"
    );
    let soft_labels: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_labels WHERE issue_id = $1 AND deleted_at IS NOT NULL",
    )
    .bind(isolated_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(soft_labels, 0, "no label rows were replaced");

    // Absent keys leave bridges untouched.
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"assignee_ids": [member]})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"name": "no bridge touch"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(live_assignees(&pool, issue_id).await, vec![member]);

    scratch.cleanup(&pool).await;
}

type ActRow = (
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Option<Uuid>,
    Option<Uuid>,
);

async fn activities(pool: &PgPool, issue_id: Uuid) -> Vec<ActRow> {
    sqlx::query_as(
        "SELECT verb, field, comment, old_value, new_value, old_identifier, new_identifier \
         FROM issue_activities WHERE issue_id = $1 ORDER BY created_at, field",
    )
    .bind(issue_id)
    .fetch_all(pool)
    .await
    .expect("activity rows")
}

async fn activity_fields(pool: &PgPool, issue_id: Uuid) -> Vec<String> {
    activities(pool, issue_id)
        .await
        .into_iter()
        .filter_map(|r| r.1)
        .collect()
}

#[tokio::test]
async fn patch_writes_per_field_activities() {
    let st = state().await;
    let pool = pool().await;
    let mut scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "activities").await;
    let member = scratch.add_actor(&pool, Some(15), Some(15)).await;
    let label = insert_label(&pool, scratch.project_id, scratch.workspace_id, "sev1").await;
    let started = insert_state(
        &pool,
        scratch.project_id,
        scratch.workspace_id,
        "Doing",
        "started",
        false,
    )
    .await;

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({
            "name": "renamed",
            "description_html": "<p>body</p>",
            "priority": "urgent",
            "state_id": started,
            "start_date": "2026-09-01",
            "target_date": "2026-09-30",
            "assignee_ids": [member],
            "label_ids": [label],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let rows = activities(&pool, issue_id).await;
    for field in [
        "name",
        "description",
        "priority",
        "state",
        "start_date",
        "target_date",
        "assignees",
        "labels",
    ] {
        assert!(
            rows.iter().any(|r| r.1.as_deref() == Some(field)),
            "missing {field}: {rows:?}"
        );
    }
    let name_row = rows
        .iter()
        .find(|r| r.1.as_deref() == Some("name"))
        .unwrap();
    assert_eq!(name_row.0, "updated");
    assert_eq!(name_row.2, "updated the name to");
    assert_eq!(name_row.3.as_deref(), Some("activities"));
    assert_eq!(name_row.4.as_deref(), Some("renamed"));
    let state_row = rows
        .iter()
        .find(|r| r.1.as_deref() == Some("state"))
        .unwrap();
    assert_eq!(state_row.3.as_deref(), Some("Backlog"));
    assert_eq!(state_row.4.as_deref(), Some("Doing"));
    assert_eq!(state_row.5, Some(scratch.state_id));
    assert_eq!(state_row.6, Some(started));
    let date_row = rows
        .iter()
        .find(|r| r.1.as_deref() == Some("start_date"))
        .unwrap();
    assert_eq!(date_row.2, "updated the start date to ");
    assert_eq!(date_row.4.as_deref(), Some("2026-09-01"));
    let assignee_row = rows
        .iter()
        .find(|r| r.1.as_deref() == Some("assignees"))
        .unwrap();
    assert_eq!(assignee_row.2, "added assignee ");
    assert_eq!(assignee_row.6, Some(member));
    let subscriber: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_subscribers WHERE issue_id = $1 AND subscriber_id = $2 AND deleted_at IS NULL",
    )
    .bind(issue_id)
    .bind(member)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(subscriber, 1, "added assignees are subscribed");

    // Removals + clearing write the matching rows.
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"assignee_ids": [], "label_ids": [], "start_date": null})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let rows = activities(&pool, issue_id).await;
    let removed_assignee = rows.iter().find(|r| r.2 == "removed assignee ").unwrap();
    assert!(
        removed_assignee.3.is_some(),
        "old_value is the display name"
    );
    assert_eq!(removed_assignee.4.as_deref(), Some(""));
    let removed_label = rows.iter().find(|r| r.2 == "removed label ").unwrap();
    assert_eq!(removed_label.3.as_deref(), Some("sev1"));
    assert_eq!(removed_label.4.as_deref(), Some(""));
    let start_date_rows = rows
        .iter()
        .filter(|r| r.1.as_deref() == Some("start_date"))
        .count();
    assert_eq!(start_date_rows, 2, "set + cleared");

    // Description merge: two consecutive description-only patches by the same
    // actor keep one row (Django's `track_description` bumps `created_at`).
    let before = activities(&pool, issue_id).await.len();
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"description_html": "<p>body 2</p>"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(activities(&pool, issue_id).await.len(), before + 1);
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"description_html": "<p>body 3</p>"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        activities(&pool, issue_id).await.len(),
        before + 1,
        "same actor merges into the previous description row"
    );

    // Different actor → new row.
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        member,
        issue_id,
        patch(json!({"description_html": "<p>body 4</p>"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(activities(&pool, issue_id).await.len(), before + 2);

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_skip_activity_suppresses_activities_and_versions() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "skip").await;

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"description_html": "<p>migrated</p>", "skip_activity": "true"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(activity_fields(&pool, issue_id).await.is_empty());
    assert_eq!(
        issue_row(&pool, issue_id).await.description_html,
        "<p>migrated</p>"
    );

    // `skip_activity` without `description_html` is ignored (Django
    // `is_description_update` gate).
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"name": "still logged", "skip_activity": "true"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        activity_fields(&pool, issue_id).await,
        vec!["name".to_string()]
    );

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn patch_estimate_activity_field_uses_estimate_type() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let issue_id = create_issue(&st, &scratch, "estimate").await;
    let estimate_point =
        insert_estimate_with_point(&pool, scratch.project_id, scratch.workspace_id).await;

    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"estimate_point": estimate_point})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let rows = activities(&pool, issue_id).await;
    let row = rows
        .iter()
        .find(|r| r.1.as_deref() == Some("estimate_points"))
        .expect("estimate_points activity");
    assert_eq!(row.4.as_deref(), Some("1"));
    assert_eq!(row.6, Some(estimate_point));

    // Change to a second point on the same estimate → old_value is the previous point value.
    let second_point = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO estimate_points (id, estimate_id, key, value, description, project_id, workspace_id, created_at, updated_at) \
         SELECT $1, estimate_id, 1, '2', '', project_id, workspace_id, now(), now() FROM estimate_points WHERE id = $2",
    )
    .bind(second_point)
    .bind(estimate_point)
    .execute(&pool)
    .await
    .unwrap();
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"estimate_point": second_point})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let rows = activities(&pool, issue_id).await;
    let change = rows
        .iter()
        .rev()
        .find(|r| r.1.as_deref() == Some("estimate_points"))
        .expect("estimate change row");
    assert_eq!(
        change.3.as_deref(),
        Some("1"),
        "old_value is the previous point value"
    );
    assert_eq!(change.4.as_deref(), Some("2"));
    assert_eq!(change.5, Some(estimate_point));
    assert_eq!(change.6, Some(second_point));

    // Clearing the estimate writes no estimate row (Django's task NPEs and
    // loses the whole batch; documented deviation 6) but other fields still log.
    let (status, _) = patch_issue_req(
        &st,
        &scratch,
        scratch.user_id,
        issue_id,
        patch(json!({"estimate_point": null, "priority": "low"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let rows = activities(&pool, issue_id).await;
    let estimate_rows = rows
        .iter()
        .filter(|r| r.1.as_deref() == Some("estimate_points"))
        .count();
    assert_eq!(estimate_rows, 2, "clearing writes no estimate row");
    assert!(rows
        .iter()
        .any(|r| r.1.as_deref() == Some("priority") && r.4.as_deref() == Some("low")));

    scratch.cleanup(&pool).await;
}
