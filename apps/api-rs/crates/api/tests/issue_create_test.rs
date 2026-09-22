//! Regression tests for the legacy issue-create handler
//! (`issue_write::create`, backing both `/issues/` and `/work-items/`):
//! per-project sequence allocation and PROJECT-level ADMIN/MEMBER authz.

use api::middleware::auth::AuthUser;
use api::routes::draft::{
    create as draft_create, create_draft_to_issue as draft_convert, ConvertBody, CreateDraftBody,
};
use api::routes::intake::{
    create_issue as intake_create_issue, CreateIntakeIssue, IntakeIssuePayload,
};
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

        let slug = format!("itseq-{}", Uuid::new_v4().simple());
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
        sqlx::query("DELETE FROM draft_issues WHERE project_id = $1")
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

/// Drop leftovers from earlier failed runs so scratch slugs never collide.
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

async fn create_body(
    st: &AppState,
    scratch: &Scratch,
    actor: Uuid,
    body: CreateIssue,
) -> (StatusCode, Value) {
    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(actor),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(body),
    )
    .await
    .expect("handler must return a response");
    (status, serde_json::to_value(body).expect("json body"))
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

async fn set_default_assignee(pool: &PgPool, project_id: Uuid, user_id: Uuid) {
    sqlx::query("UPDATE projects SET default_assignee_id = $2 WHERE id = $1")
        .bind(project_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("set default assignee");
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
        sqlx::query_scalar("SELECT id FROM workspaces WHERE slug LIKE 'itseq-%'")
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
                "DELETE FROM draft_issues WHERE project_id = $1",
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
    let _ = sqlx::query("DELETE FROM users WHERE username LIKE 'itseq-%'")
        .execute(pool)
        .await;
}

async fn sequence_of(pool: &PgPool, issue_id: Uuid) -> i32 {
    sqlx::query_scalar("SELECT sequence_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_one(pool)
        .await
        .expect("issue row")
}

async fn sequence_rows(pool: &PgPool, issue_id: Uuid) -> Vec<i64> {
    sqlx::query_scalar("SELECT sequence FROM issue_sequences WHERE issue_id = $1")
        .bind(issue_id)
        .fetch_all(pool)
        .await
        .expect("sequence rows")
}

#[tokio::test]
async fn create_allocates_distinct_sequences_and_records_counter_rows() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    let body = |name: &str| base_body(name, scratch.state_id);

    let (s1, Json(first)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(body("seq-probe-a")),
    )
    .await
    .expect("first create must succeed");
    assert_eq!(s1, axum::http::StatusCode::CREATED);
    let first_id = Uuid::parse_str(first["id"].as_str().expect("id")).unwrap();

    let (s2, Json(second)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(body("seq-probe-b")),
    )
    .await
    .expect("second create must succeed");
    assert_eq!(s2, axum::http::StatusCode::CREATED);
    let second_id = Uuid::parse_str(second["id"].as_str().expect("id")).unwrap();

    let seq1 = sequence_of(&st.pool, first_id).await;
    let seq2 = sequence_of(&st.pool, second_id).await;
    assert_ne!(seq1, seq2, "consecutive creates must not reuse a sequence");

    assert_eq!(
        sequence_rows(&st.pool, first_id).await,
        vec![seq1 as i64],
        "create must persist the issue_sequences counter row"
    );
    assert_eq!(
        sequence_rows(&st.pool, second_id).await,
        vec![seq2 as i64],
        "create must persist the issue_sequences counter row"
    );

    let created_by: Option<Uuid> =
        sqlx::query_scalar("SELECT created_by_id FROM issues WHERE id = $1")
            .bind(first_id)
            .fetch_one(&st.pool)
            .await
            .unwrap();
    assert_eq!(
        created_by,
        Some(scratch.user_id),
        "creator must be recorded"
    );

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn intake_create_allocates_distinct_sequences_and_records_counter_rows() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    sqlx::query(
        "INSERT INTO intakes (id, name, description, is_default, view_props, logo_props, \
         project_id, workspace_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), 'Intake', '', true, '{}'::jsonb, '{}'::jsonb, $1, $2, now(), now())",
    )
    .bind(scratch.project_id)
    .bind(scratch.workspace_id)
    .execute(&st.pool)
    .await
    .expect("scratch intake");

    let body = |name: &str| CreateIntakeIssue {
        issue: IntakeIssuePayload {
            name: Some(name.to_string()),
            priority: Some("none".to_string()),
        },
    };

    let (s1, Json(_)) = intake_create_issue(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(body("intake-probe-a")),
    )
    .await
    .expect("first intake create must succeed");
    assert_eq!(s1, axum::http::StatusCode::OK);

    let (s2, Json(_)) = intake_create_issue(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(body("intake-probe-b")),
    )
    .await
    .expect("second intake create must succeed");
    assert_eq!(s2, axum::http::StatusCode::OK);

    let ids: Vec<(Uuid, i32)> = sqlx::query_as(
        "SELECT id, sequence_id FROM issues WHERE project_id = $1 ORDER BY created_at",
    )
    .bind(scratch.project_id)
    .fetch_all(&st.pool)
    .await
    .unwrap();
    assert_eq!(ids.len(), 2, "both intake creates must persist an issue");
    assert_ne!(
        ids[0].1, ids[1].1,
        "consecutive creates must not reuse a sequence"
    );
    let (stripped, completed): (Option<String>, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT description_stripped, completed_at FROM issues WHERE id = $1")
            .bind(ids[0].0)
            .fetch_one(&st.pool)
            .await
            .unwrap();
    assert_eq!(
        stripped.as_deref(),
        Some(""),
        "default html → stripped \"\""
    );
    assert_eq!(completed, None, "triage state is not completed");
    for (issue_id, sequence) in ids {
        assert_eq!(
            sequence_rows(&st.pool, issue_id).await,
            vec![sequence as i64],
            "intake create must persist the issue_sequences counter row"
        );
    }

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn no_active_issue_shares_a_sequence_within_a_project() {
    let pool = pool().await;
    let duplicates: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ( \
           SELECT project_id, sequence_id FROM issues WHERE deleted_at IS NULL \
           GROUP BY project_id, sequence_id HAVING COUNT(*) > 1 \
         ) dup",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        duplicates, 0,
        "active issues must have unique per-project sequences"
    );
}

#[tokio::test]
async fn every_active_issue_has_a_matching_sequence_row() {
    let pool = pool().await;
    let missing: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issues i WHERE i.deleted_at IS NULL \
         AND NOT EXISTS (SELECT 1 FROM issue_sequences sq WHERE sq.issue_id = i.id)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        missing, 0,
        "every active issue must have an issue_sequences row"
    );

    let lagging: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM projects p WHERE \
         COALESCE((SELECT MAX(sequence_id) FROM issues i \
                   WHERE i.project_id = p.id AND i.deleted_at IS NULL), 0) \
         > COALESCE((SELECT MAX(sequence) FROM issue_sequences sq \
                     WHERE sq.project_id = p.id), 0)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        lagging, 0,
        "issue_sequences counter must cover the highest issue sequence"
    );
}

#[tokio::test]
async fn unique_index_rejects_duplicate_active_sequence() {
    let pool = pool().await;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_indexes WHERE indexname = \
         'issue_unique_project_sequence_active')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        exists,
        "partial unique index on active (project_id, sequence_id) must exist"
    );

    let source: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM issues WHERE deleted_at IS NULL LIMIT 1")
            .fetch_optional(&pool)
            .await
            .unwrap();
    let Some(source) = source else {
        return;
    };

    let mut tx = pool.begin().await.unwrap();
    let result = sqlx::query(
        "INSERT INTO issues (id, name, description_html, description_json, priority, is_draft, \
         sort_order, sequence_id, state_id, project_id, workspace_id, created_at, updated_at) \
         SELECT gen_random_uuid(), name, description_html, description_json, priority, is_draft, \
         sort_order, sequence_id, state_id, project_id, workspace_id, now(), now() \
         FROM issues WHERE id = $1",
    )
    .bind(source)
    .execute(&mut *tx)
    .await;
    assert!(
        result.is_err(),
        "duplicate active sequence must be rejected by the unique index"
    );
    let _ = tx.rollback().await;
}

async fn create_as(
    st: &AppState,
    scratch: &Scratch,
    actor: Uuid,
    name: &str,
) -> (StatusCode, Value) {
    create_body(st, scratch, actor, base_body(name, scratch.state_id)).await
}

#[tokio::test]
async fn outsider_create_is_forbidden_without_insert() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let outsider = scratch.add_actor(&st.pool, None, None).await;

    let (status, body) = create_as(&st, &scratch, outsider, "outsider-probe").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        body,
        json!({"error": "You don't have the required permissions."})
    );

    let persisted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE project_id = $1")
        .bind(scratch.project_id)
        .fetch_one(&st.pool)
        .await
        .unwrap();
    assert_eq!(persisted, 0, "forbidden create must not insert an issue");

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn gate_runs_before_body_validation() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let outsider = scratch.add_actor(&st.pool, None, None).await;

    let result = create(
        State(st.clone()),
        AuthUser(outsider),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(CreateIssue {
            name: String::new(),
            assignee_ids: None,
            label_ids: None,
            state_id: None,
            description_html: None,
            priority: None,
            start_date: None,
            target_date: None,
            parent_id: None,
            type_id: None,
            estimate_point: None,
        }),
    )
    .await;

    match result {
        Ok((status, Json(body))) => {
            assert_eq!(
                status,
                StatusCode::FORBIDDEN,
                "gate must win over validation"
            );
            assert_eq!(
                serde_json::to_value(body).unwrap(),
                json!({"error": "You don't have the required permissions."})
            );
        }
        Err(e) => panic!("expected 403 before validation, got handler error: {e:?}"),
    }

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn guest_project_member_is_forbidden() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let guest = scratch.add_actor(&st.pool, Some(5), Some(5)).await;

    let (status, body) = create_as(&st, &scratch, guest, "guest-probe").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        body,
        json!({"error": "You don't have the required permissions."})
    );

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn project_member_role_15_can_create() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let member = scratch.add_actor(&st.pool, Some(15), Some(15)).await;

    let (status, body) = create_as(&st, &scratch, member, "member-probe").await;
    assert_eq!(status, StatusCode::CREATED);

    let id = Uuid::parse_str(body["id"].as_str().expect("id in body")).unwrap();
    let created_by: Option<Uuid> =
        sqlx::query_scalar("SELECT created_by_id FROM issues WHERE id = $1")
            .bind(id)
            .fetch_one(&st.pool)
            .await
            .unwrap();
    assert_eq!(created_by, Some(member));

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn workspace_admin_with_any_project_membership_can_create() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let admin = scratch.add_actor(&st.pool, Some(20), Some(5)).await;

    let (status, _) = create_as(&st, &scratch, admin, "ws-admin-probe").await;
    assert_eq!(status, StatusCode::CREATED);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn workspace_admin_without_project_membership_is_forbidden() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let admin = scratch.add_actor(&st.pool, Some(20), None).await;

    let (status, body) = create_as(&st, &scratch, admin, "ws-admin-outsider-probe").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        body,
        json!({"error": "You don't have the required permissions."})
    );

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_persists_extended_fields() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    let (parent_status, parent_body) =
        create_as(&st, &scratch, scratch.user_id, "parent-probe").await;
    assert_eq!(parent_status, StatusCode::CREATED);
    let parent_id = Uuid::parse_str(parent_body["id"].as_str().expect("id")).unwrap();

    let mut body = base_body("extended-probe", scratch.state_id);
    body.description_html = Some("<p>hello world</p>".to_string());
    body.priority = Some("high".to_string());
    body.start_date = Some("2026-09-01".to_string());
    body.target_date = Some("2026-09-30".to_string());
    body.parent_id = Some(parent_id);

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();

    let row: (
        String,
        String,
        Option<String>,
        Option<String>,
        Option<Uuid>,
        Option<Uuid>,
    ) = sqlx::query_as(
        "SELECT description_html, priority, start_date::text, target_date::text, parent_id, \
             updated_by_id FROM issues WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&st.pool)
    .await
    .unwrap();
    assert_eq!(row.0, "<p>hello world</p>");
    assert_eq!(row.1, "high");
    assert_eq!(row.2.as_deref(), Some("2026-09-01"));
    assert_eq!(row.3.as_deref(), Some("2026-09-30"));
    assert_eq!(row.4, Some(parent_id));
    assert_eq!(
        row.5, None,
        "updated_by must stay NULL on create (Django parity)"
    );

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_persists_type_and_estimate_point() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let type_id = insert_issue_type(&st.pool, scratch.workspace_id, "Bug").await;
    let point_id =
        insert_estimate_with_point(&st.pool, scratch.project_id, scratch.workspace_id).await;

    let mut body = base_body("typed-probe", scratch.state_id);
    body.type_id = Some(type_id);
    body.estimate_point = Some(point_id);

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();

    let row: (Option<Uuid>, Option<Uuid>) =
        sqlx::query_as("SELECT type_id, estimate_point_id FROM issues WHERE id = $1")
            .bind(id)
            .fetch_one(&st.pool)
            .await
            .unwrap();
    assert_eq!(row.0, Some(type_id));
    assert_eq!(row.1, Some(point_id));

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_defaults_description_and_priority_when_absent() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    let (status, payload) = create_body(
        &st,
        &scratch,
        scratch.user_id,
        base_body("defaults-probe", scratch.state_id),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();

    let row: (
        String,
        String,
        Option<String>,
        Option<Uuid>,
        Option<Uuid>,
        Option<Uuid>,
    ) = sqlx::query_as(
        "SELECT description_html, priority, description_stripped, parent_id, type_id, \
             estimate_point_id FROM issues WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&st.pool)
    .await
    .unwrap();
    assert_eq!(row.0, "<p></p>");
    assert_eq!(row.1, "none");
    // `Issue.save` create branch: non-empty html → `strip_tags` ("" here).
    assert_eq!(row.2.as_deref(), Some(""));
    assert_eq!(row.3, None);
    assert_eq!(row.4, None);
    assert_eq!(row.5, None);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_validation_errors_return_400() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    let cases: Vec<(&str, CreateIssue, &str)> = vec![
        (
            "blank-name",
            base_body("", scratch.state_id),
            "name is required",
        ),
        (
            "unknown-assignee",
            {
                let mut b = base_body("unknown-assignee", scratch.state_id);
                b.assignee_ids = Some(vec![Uuid::new_v4()]);
                b
            },
            "invalid assignee: not a project member",
        ),
        (
            "unknown-label",
            {
                let mut b = base_body("unknown-label", scratch.state_id);
                b.label_ids = Some(vec![Uuid::new_v4()]);
                b
            },
            "invalid label: not in project",
        ),
        (
            "unknown-state",
            {
                let mut b = base_body("unknown-state", scratch.state_id);
                b.state_id = Some(Uuid::new_v4());
                b
            },
            "State is not valid please pass a valid state_id",
        ),
        (
            "unknown-type",
            {
                let mut b = base_body("unknown-type", scratch.state_id);
                b.type_id = Some(Uuid::new_v4());
                b
            },
            "type_id is not valid",
        ),
        (
            "unknown-parent",
            {
                let mut b = base_body("unknown-parent", scratch.state_id);
                b.parent_id = Some(Uuid::new_v4());
                b
            },
            "parent is not valid",
        ),
        (
            "unknown-estimate",
            {
                let mut b = base_body("unknown-estimate", scratch.state_id);
                b.estimate_point = Some(Uuid::new_v4());
                b
            },
            "estimate_point is not valid",
        ),
        (
            "bad-date",
            {
                let mut b = base_body("bad-date", scratch.state_id);
                b.start_date = Some("09/01/2026".to_string());
                b
            },
            "Invalid date: 09/01/2026",
        ),
    ];

    for (case, body, expected) in cases {
        let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "case {case} must 400");
        assert_eq!(payload, json!({"error": expected}), "case {case} body");
    }

    let persisted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE project_id = $1")
        .bind(scratch.project_id)
        .fetch_one(&st.pool)
        .await
        .unwrap();
    assert_eq!(persisted, 0, "failed validation must not insert an issue");

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn invalid_priority_returns_400() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    let mut body = base_body("bad-priority", scratch.state_id);
    body.priority = Some("critical".to_string());

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(payload, json!({"error": "Invalid priority"}));

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_writes_assignee_and_label_bridges() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let member = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    let label = insert_label(&st.pool, scratch.project_id, scratch.workspace_id, "urgent").await;

    let mut body = base_body("bridge-probe", scratch.state_id);
    body.assignee_ids = Some(vec![member]);
    body.label_ids = Some(vec![label]);

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();

    let rows: Vec<(Uuid, Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
        "SELECT assignee_id, created_by_id, updated_by_id FROM issue_assignees \
         WHERE issue_id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1, "exactly one live assignee bridge");
    assert_eq!(rows[0].0, member);
    assert_eq!(rows[0].1, Some(scratch.user_id));
    assert_eq!(rows[0].2, None, "bridge updated_by_id stays NULL on create");

    let labels: Vec<Uuid> = sqlx::query_scalar(
        "SELECT label_id FROM issue_labels WHERE issue_id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await
    .unwrap();
    assert_eq!(labels, vec![label]);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_applies_default_assignee_when_absent() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let default_user = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    set_default_assignee(&st.pool, scratch.project_id, default_user).await;

    let (status, payload) = create_body(
        &st,
        &scratch,
        scratch.user_id,
        base_body("default-absent", scratch.state_id),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();
    assert_eq!(live_assignees(&st.pool, id).await, vec![default_user]);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_applies_default_assignee_when_empty() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let default_user = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    set_default_assignee(&st.pool, scratch.project_id, default_user).await;

    let mut body = base_body("default-empty", scratch.state_id);
    body.assignee_ids = Some(vec![]);

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();
    assert_eq!(live_assignees(&st.pool, id).await, vec![default_user]);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_ignores_default_assignee_when_assignees_sent() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let default_user = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    let chosen = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    set_default_assignee(&st.pool, scratch.project_id, default_user).await;

    let mut body = base_body("default-ignored", scratch.state_id);
    body.assignee_ids = Some(vec![chosen]);

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();
    assert_eq!(live_assignees(&st.pool, id).await, vec![chosen]);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_skips_ineligible_default_assignee() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let guest = scratch.add_actor(&st.pool, Some(15), Some(5)).await;
    set_default_assignee(&st.pool, scratch.project_id, guest).await;

    let (status, payload) = create_body(
        &st,
        &scratch,
        scratch.user_id,
        base_body("default-ineligible", scratch.state_id),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();
    assert!(
        live_assignees(&st.pool, id).await.is_empty(),
        "role < 15 default assignee must not be bridged"
    );

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn duplicate_assignee_ids_are_deduped() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let member = scratch.add_actor(&st.pool, Some(15), Some(15)).await;

    let mut body = base_body("dedupe-probe", scratch.state_id);
    body.assignee_ids = Some(vec![member, member]);

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();
    assert_eq!(
        live_assignees(&st.pool, id).await,
        vec![member],
        "duplicate ids must collapse to one bridge row"
    );

    scratch.cleanup(&st.pool).await;
}

type CreatedActRow = (
    String,
    Option<String>,
    String,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
);
type AssigneeActRow = (String, String, String, String, Option<Uuid>, Option<Uuid>);

#[tokio::test]
async fn create_writes_created_activity_and_assignee_artifacts() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let member = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    let display_name: String = sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1")
        .bind(member)
        .fetch_one(&st.pool)
        .await
        .unwrap();

    let mut body = base_body("activity-probe", scratch.state_id);
    body.assignee_ids = Some(vec![member]);

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();

    let created: Vec<CreatedActRow> =
        sqlx::query_as(
            "SELECT verb, field, comment, actor_id, created_by_id, updated_by_id FROM issue_activities \
             WHERE issue_id = $1 AND verb = 'created'",
        )
        .bind(id)
        .fetch_all(&st.pool)
        .await
        .unwrap();
    assert_eq!(created.len(), 1, "exactly one created activity row");
    assert_eq!(created[0].0, "created");
    assert_eq!(created[0].1, None);
    assert_eq!(created[0].2, "created the issue");
    assert_eq!(created[0].3, Some(scratch.user_id));
    assert_eq!(created[0].4, Some(scratch.user_id));
    assert_eq!(created[0].5, None);

    let assignee_acts: Vec<AssigneeActRow> = sqlx::query_as(
        "SELECT field, old_value, new_value, comment, new_identifier, created_by_id \
              FROM issue_activities WHERE issue_id = $1 AND field = 'assignees'",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await
    .unwrap();
    assert_eq!(assignee_acts.len(), 1);
    assert_eq!(assignee_acts[0].0, "assignees");
    assert_eq!(assignee_acts[0].1, "");
    assert_eq!(assignee_acts[0].2, display_name);
    assert_eq!(assignee_acts[0].3, "added assignee ");
    assert_eq!(assignee_acts[0].4, Some(member));
    assert_eq!(
        assignee_acts[0].5, None,
        "bulk_create parity leaves created_by_id NULL"
    );

    let subs: Vec<(Uuid, Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
        "SELECT subscriber_id, created_by_id, updated_by_id FROM issue_subscribers \
         WHERE issue_id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await
    .unwrap();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].0, member);
    assert_eq!(
        subs[0].1,
        Some(member),
        "subscriber row is authored by the assignee"
    );
    assert_eq!(subs[0].2, Some(member));

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn default_assignee_gets_no_activity_or_subscriber() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let default_user = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    set_default_assignee(&st.pool, scratch.project_id, default_user).await;

    let (status, payload) = create_body(
        &st,
        &scratch,
        scratch.user_id,
        base_body("default-activity", scratch.state_id),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();

    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
         (SELECT COUNT(*) FROM issue_activities WHERE issue_id = $1 AND field = 'assignees'), \
         (SELECT COUNT(*) FROM issue_subscribers WHERE issue_id = $1 AND deleted_at IS NULL), \
         (SELECT COUNT(*) FROM issue_activities WHERE issue_id = $1 AND verb = 'created')",
    )
    .bind(id)
    .fetch_one(&st.pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0, 1), "default assignee is not tracked");

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn labels_get_no_activity_rows() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let label = insert_label(&st.pool, scratch.project_id, scratch.workspace_id, "bug").await;

    let mut body = base_body("label-activity", scratch.state_id);
    body.label_ids = Some(vec![label]);

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();

    let label_activities: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_activities WHERE issue_id = $1 AND field = 'labels'",
    )
    .bind(id)
    .fetch_one(&st.pool)
    .await
    .unwrap();
    assert_eq!(label_activities, 0, "Django has no track_labels on create");

    let created_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_activities WHERE issue_id = $1 AND verb = 'created'",
    )
    .bind(id)
    .fetch_one(&st.pool)
    .await
    .unwrap();
    assert_eq!(created_count, 1);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_response_has_26_key_list_shape() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let member = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    let label = insert_label(&st.pool, scratch.project_id, scratch.workspace_id, "urgent").await;

    let mut body = base_body("response-probe", scratch.state_id);
    body.assignee_ids = Some(vec![member]);
    body.label_ids = Some(vec![label]);
    body.priority = Some("medium".to_string());
    body.description_html = Some("<p>resp</p>".to_string());

    let (status, payload) = create_body(&st, &scratch, scratch.user_id, body).await;
    assert_eq!(status, StatusCode::CREATED);

    let obj = payload.as_object().expect("response must be a JSON object");
    assert_eq!(obj.len(), 26, "response must have exactly the 26 list keys");
    for key in [
        "id",
        "name",
        "state_id",
        "sort_order",
        "completed_at",
        "estimate_point",
        "priority",
        "start_date",
        "target_date",
        "sequence_id",
        "project_id",
        "parent_id",
        "cycle_id",
        "module_ids",
        "label_ids",
        "assignee_ids",
        "sub_issues_count",
        "created_at",
        "updated_at",
        "created_by",
        "updated_by",
        "attachment_count",
        "link_count",
        "is_draft",
        "archived_at",
        "deleted_at",
    ] {
        assert!(obj.contains_key(key), "missing response key: {key}");
    }
    for absent in ["description_html", "type_id"] {
        assert!(
            !obj.contains_key(absent),
            "unexpected response key: {absent}"
        );
    }

    let id = Uuid::parse_str(payload["id"].as_str().expect("id")).unwrap();
    let sequence_id = sequence_of(&st.pool, id).await;
    assert_eq!(payload["name"], "response-probe");
    assert_eq!(payload["priority"], "medium");
    assert_eq!(
        payload["state_id"].as_str().unwrap(),
        scratch.state_id.to_string()
    );
    assert_eq!(
        payload["project_id"].as_str().unwrap(),
        scratch.project_id.to_string()
    );
    assert_eq!(payload["sequence_id"].as_i64(), Some(sequence_id as i64));
    assert_eq!(payload["assignee_ids"], json!([member.to_string()]));
    assert_eq!(payload["label_ids"], json!([label.to_string()]));
    assert_eq!(payload["module_ids"], json!([]));
    assert_eq!(payload["sub_issues_count"], 0);
    assert_eq!(payload["attachment_count"], 0);
    assert_eq!(payload["link_count"], 0);
    assert_eq!(payload["is_draft"], false);
    assert!(payload["updated_by"].is_null());
    assert_eq!(
        payload["created_by"].as_str().unwrap(),
        scratch.user_id.to_string()
    );
    assert!(payload["archived_at"].is_null());
    assert!(payload["deleted_at"].is_null());

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_records_initial_description_version() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;

    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(CreateIssue {
            name: "versioned create".to_string(),
            description_html: Some("<p>born</p>".to_string()),
            ..base_body("unused", scratch.state_id)
        }),
    )
    .await
    .expect("create must return a response");
    assert_eq!(status, StatusCode::CREATED);
    let issue_id: Uuid = body["id"].as_str().unwrap().parse().unwrap();

    let rows: Vec<(String, Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
        "SELECT description_html, owned_by_id, updated_by_id FROM issue_description_versions WHERE issue_id = $1",
    )
    .bind(issue_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "<p>born</p>");
    assert_eq!(rows[0].1, Some(scratch.user_id));
    assert_eq!(rows[0].2, None, "create leaves updated_by NULL");

    sqlx::query("DELETE FROM issue_description_versions WHERE issue_id = $1")
        .bind(issue_id)
        .execute(&pool)
        .await
        .ok();
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn create_writes_stripped_and_completed_at() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;

    let done = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO states (id, name, description, color, slug, project_id, workspace_id, sequence, \
         \"group\", \"default\", is_triage, created_at, updated_at) \
         VALUES ($1, 'Done', '', '#16A34A', 'done', $2, $3, 65535, 'completed', false, false, now(), now())",
    )
    .bind(done)
    .bind(scratch.project_id)
    .bind(scratch.workspace_id)
    .execute(&pool)
    .await
    .unwrap();

    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(CreateIssue {
            state_id: Some(done),
            description_html: Some("<p>hi <b>there</b></p>".to_string()),
            ..base_body("completed-create", scratch.state_id)
        }),
    )
    .await
    .expect("create must return a response");
    assert_eq!(status, StatusCode::CREATED);
    let id: Uuid = body["id"].as_str().unwrap().parse().unwrap();

    let (stripped, completed): (Option<String>, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT description_stripped, completed_at FROM issues WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stripped.as_deref(), Some("hi there"));
    assert!(
        completed.is_some(),
        "create into a completed state stamps completed_at"
    );

    // Non-completed state (fixture backlog) → completed_at stays NULL.
    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(base_body("backlog-create", scratch.state_id)),
    )
    .await
    .expect("create must return a response");
    assert_eq!(status, StatusCode::CREATED);
    let id: Uuid = body["id"].as_str().unwrap().parse().unwrap();
    let completed: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT completed_at FROM issues WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(completed, None);

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn insert_issue_handles_null_state_and_empty_html() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;

    let mut tx = pool.begin().await.unwrap();
    let out = api::routes::issue_write::insert_issue(
        &mut tx,
        api::routes::issue_write::NewIssue {
            slug: &scratch.slug,
            project_id: scratch.project_id,
            state_id: None,
            name: "null-state",
            description_html: "",
            priority: "none",
            start_date: None,
            target_date: None,
            parent_id: None,
            type_id: None,
            estimate_point_id: None,
            created_by: scratch.user_id,
        },
    )
    .await
    .expect("insert_issue");
    tx.commit().await.unwrap();

    let (html, stripped, completed): (
        String,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT description_html, description_stripped, completed_at FROM issues WHERE id = $1",
    )
    .bind(out.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(html, "", "empty html stored as-is");
    assert_eq!(stripped, None, "empty html → NULL stripped");
    assert_eq!(completed, None, "NULL state → no completed_at");

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn draft_default_state_uses_state_sequence_ordering() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;

    // Fixture state: sequence 65535, default=true, created FIRST. A second
    // default with a LOWER sequence but created LATER must win — Django
    // resolves via `State.objects.filter(...).first()` with
    // `Meta.ordering = ("sequence",)` (`db/models/state.py:115`,
    // `db/models/draft.py:84-98`).
    let second = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO states (id, name, description, color, slug, project_id, workspace_id, sequence, \
         \"group\", \"default\", is_triage, created_at, updated_at) \
         VALUES ($1, 'First by sequence', '', '#60646C', 'first-by-seq', $2, $3, 100, 'backlog', true, false, now(), now())",
    )
    .bind(second)
    .bind(scratch.project_id)
    .bind(scratch.workspace_id)
    .execute(&pool)
    .await
    .unwrap();

    let (status, Json(_)) = draft_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Some(Json(CreateDraftBody {
            project_id: Some(scratch.project_id),
            ..Default::default()
        })),
    )
    .await
    .expect("draft create must respond");
    assert_eq!(status, StatusCode::CREATED);

    let state_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT state_id FROM draft_issues WHERE project_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(scratch.project_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        state_id,
        Some(second),
        "sequence ordering must win over created_at"
    );

    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn draft_conversion_writes_description_stripped() {
    let st = state().await;
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;

    let draft_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO draft_issues (id, name, description_html, description_json, priority, sort_order, \
         state_id, project_id, workspace_id, created_by_id, created_at, updated_at) \
         VALUES ($1, 'draft-probe', '<p>draft</p>', '{}', 'none', 65535, $2, $3, $4, $5, now(), now())",
    )
    .bind(draft_id)
    .bind(scratch.state_id)
    .bind(scratch.project_id)
    .bind(scratch.workspace_id)
    .bind(scratch.user_id)
    .execute(&pool)
    .await
    .unwrap();

    let (status, Json(body)) = draft_convert(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), draft_id)),
        Some(Json(ConvertBody {
            name: Some("converted-probe".to_string()),
            description_html: Some("<p>converted <b>body</b></p>".to_string()),
            ..Default::default()
        })),
    )
    .await
    .expect("draft conversion must respond");
    assert_eq!(status, StatusCode::CREATED);
    let issue_id: Uuid = body["id"].as_str().expect("id").parse().unwrap();

    let stripped: Option<String> =
        sqlx::query_scalar("SELECT description_stripped FROM issues WHERE id = $1")
            .bind(issue_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stripped.as_deref(), Some("converted body"));

    scratch.cleanup(&pool).await;
}
