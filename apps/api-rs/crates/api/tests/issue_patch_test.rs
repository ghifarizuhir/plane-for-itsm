//! Regression tests for the legacy issue PATCH handler
//! (`issue_update::patch_issue`): full web payload persistence, Django
//! serializer validation, bridges, update activities and description
//! versions.

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
