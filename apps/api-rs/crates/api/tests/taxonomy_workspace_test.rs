//! Workspace-level taxonomy (program P1/S3). Run with `--test-threads=1`.
//!
//! Labels section here; estimates (Task 3) and work item types (Task 4)
//! append their cases to this file.

use api::middleware::auth::AuthUser;
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
    admin_id: Uuid,
    member_id: Uuid,
    guest_id: Uuid,
}

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let slug = format!("wst-{}", Uuid::new_v4().simple());
        let admin_id = Uuid::new_v4();
        let member_id = Uuid::new_v4();
        let guest_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        insert_user(pool, admin_id, &format!("{slug}-admin")).await;
        insert_user(pool, member_id, &format!("{slug}-member")).await;
        insert_user(pool, guest_id, &format!("{slug}-guest")).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'Wst', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
        )
        .bind(workspace_id)
        .bind(&slug)
        .bind(admin_id)
        .execute(pool)
        .await
        .expect("scratch workspace");
        for (user_id, role) in [(admin_id, 20), (member_id, 15), (guest_id, 5)] {
            sqlx::query(
                "INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, \
                 workspace_id, view_props, default_props, issue_props, explored_features, \
                 getting_started_checklist, tips, is_active) \
                 VALUES (gen_random_uuid(), now(), now(), $1, $2, $3, '{}', '{}', '{}', '{}', \
                 '{}', '{}', true)",
            )
            .bind(role)
            .bind(user_id)
            .bind(workspace_id)
            .execute(pool)
            .await
            .expect("scratch workspace member");
        }
        Self {
            slug,
            workspace_id,
            admin_id,
            member_id,
            guest_id,
        }
    }

    async fn insert_project(&self, pool: &PgPool, name: &str, identifier: &str) -> Uuid {
        let project_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO projects (id, name, description, identifier, workspace_id, network, \
             module_view, cycle_view, issue_views_view, page_view, intake_view, \
             is_time_tracking_enabled, is_issue_type_enabled, guest_view_all_features, \
             archive_in, close_in, logo_props, timezone, created_by_id, updated_by_id, \
             created_at, updated_at) \
             VALUES ($1, $2, '', $3, $4, 2, true, true, true, true, false, false, false, \
             false, 0, 0, '{}', 'UTC', $5, $5, now(), now())",
        )
        .bind(project_id)
        .bind(name)
        .bind(identifier)
        .bind(self.workspace_id)
        .bind(self.admin_id)
        .execute(pool)
        .await
        .expect("scratch project");
        project_id
    }

    async fn insert_label(
        &self,
        pool: &PgPool,
        project_id: Option<Uuid>,
        name: &str,
        parent_id: Option<Uuid>,
    ) -> Uuid {
        let label_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO labels (id, name, color, description, project_id, workspace_id, \
             parent_id, sort_order, created_by_id, updated_by_id, created_at, updated_at) \
             VALUES ($1, $2, 'red', '', $3, $4, $5, 65535, $6, $6, now(), now())",
        )
        .bind(label_id)
        .bind(name)
        .bind(project_id)
        .bind(self.workspace_id)
        .bind(parent_id)
        .bind(self.admin_id)
        .execute(pool)
        .await
        .expect("scratch label");
        label_id
    }

    async fn insert_issue(&self, pool: &PgPool, project_id: Uuid, name: &str) -> Uuid {
        let issue_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO issues (id, name, description_html, description_json, is_draft, \
             priority, project_id, sequence_id, sort_order, workspace_id, created_by_id, \
             updated_by_id, created_at, updated_at) \
             VALUES ($1, $2, '', '{}', false, 'none', $3, 1, 0, $4, $5, $5, now(), now())",
        )
        .bind(issue_id)
        .bind(name)
        .bind(project_id)
        .bind(self.workspace_id)
        .bind(self.admin_id)
        .execute(pool)
        .await
        .expect("scratch issue");
        issue_id
    }

    async fn purge(&self, pool: &PgPool) {
        for statement in [
            "DELETE FROM issue_labels WHERE workspace_id = $1",
            "DELETE FROM issues WHERE workspace_id = $1",
            "DELETE FROM labels WHERE workspace_id = $1",
            "DELETE FROM estimate_points WHERE workspace_id = $1",
            "DELETE FROM estimates WHERE workspace_id = $1",
            "DELETE FROM project_issue_types WHERE workspace_id = $1",
            "DELETE FROM issue_types WHERE workspace_id = $1",
            "DELETE FROM project_issue_types WHERE workspace_id = $1",
            "DELETE FROM issue_types WHERE workspace_id = $1",
            "DELETE FROM workspace_settings WHERE workspace_id = $1",
            "DELETE FROM project_members WHERE workspace_id = $1",
            "DELETE FROM projects WHERE workspace_id = $1",
            "DELETE FROM workspace_members WHERE workspace_id = $1",
        ] {
            sqlx::query(statement)
                .bind(self.workspace_id)
                .execute(pool)
                .await
                .ok();
        }
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

#[tokio::test]
async fn labels_merge_dedupes_across_projects() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;

    let app_a = scratch.insert_project(&pool, "App A", "APPA").await;
    let app_b = scratch.insert_project(&pool, "App B", "APPB").await;
    // Same name (different case) in two projects + a child under B's copy.
    let bug_a = scratch
        .insert_label(&pool, Some(app_a), "Bug", None)
        .await;
    let bug_b = scratch
        .insert_label(&pool, Some(app_b), "bug", None)
        .await;
    let _task = scratch
        .insert_label(&pool, Some(app_a), "Task", None)
        .await;
    let child = scratch
        .insert_label(&pool, Some(app_b), "Crash", Some(bug_b))
        .await;
    let issue_id = scratch.insert_issue(&pool, app_b, "broken thing").await;
    sqlx::query(
        "INSERT INTO issue_labels (id, issue_id, label_id, project_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $5, now(), now())",
    )
    .bind(issue_id)
    .bind(bug_b)
    .bind(app_b)
    .bind(scratch.workspace_id)
    .bind(scratch.admin_id)
    .execute(&pool)
    .await
    .expect("scratch issue label");

    api::routes::taxonomy_backfill::backfill_labels(&pool, scratch.workspace_id)
        .await
        .expect("backfill labels");

    // Canonical = oldest (App A's "Bug").
    let (kept_project,): (Option<Uuid>,) =
        sqlx::query_as("SELECT project_id FROM labels WHERE id = $1")
            .bind(bug_a)
            .fetch_one(&pool)
            .await
            .expect("fetch canonical");
    assert_eq!(kept_project, None);
    let dead: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM labels WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(bug_b)
    .fetch_optional(&pool)
    .await
    .expect("fetch dupe");
    assert!(dead.is_none());
    let pointed: Uuid = sqlx::query_scalar(
        "SELECT label_id FROM issue_labels WHERE issue_id = $1 AND deleted_at IS NULL",
    )
    .bind(issue_id)
    .fetch_one(&pool)
    .await
    .expect("fetch remap");
    assert_eq!(pointed, bug_a);
    let reparented: Option<Option<Uuid>> = sqlx::query_scalar(
        "SELECT parent_id FROM labels WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(child)
    .fetch_optional(&pool)
    .await
    .expect("fetch child");
    assert_eq!(reparented, Some(Some(bug_a)));
    scratch.purge(&pool).await;
}

#[tokio::test]
async fn project_label_endpoints_serve_workspace_set() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let app_a = scratch.insert_project(&pool, "App A", "APPA").await;
    let app_b = scratch.insert_project(&pool, "App B", "APPB").await;
    scratch
        .insert_label(&pool, Some(app_a), "Shared", None)
        .await;
    api::routes::taxonomy_backfill::backfill_labels(&pool, scratch.workspace_id)
        .await
        .expect("backfill labels");

    // GET project-list returns the workspace set, including other apps' labels.
    let list = api::routes::label::list(
        State(st.clone()),
        AuthUser(scratch.admin_id),
        Path((scratch.slug.clone(), app_b)),
    )
    .await
    .expect("list labels");
    assert!(list.0.iter().any(|l| l.name == "Shared"));

    // POST duplicate name in another project → 409 workspace message.
    let (status, _) = api::routes::label::create(
        State(st.clone()),
        AuthUser(scratch.admin_id),
        Path((scratch.slug.clone(), app_b)),
        Json(api::routes::label::CreateLabel {
            name: "shared".to_string(),
            color: None,
            description: None,
            external_source: None,
            external_id: None,
            parent_id: None,
            sort_order: None,
        }),
    )
    .await
    .expect("create duplicate");
    assert_eq!(status, StatusCode::CONFLICT);

    // POST new name → 201 with NULL project_id.
    let (status, Json(body)) = api::routes::label::create(
        State(st.clone()),
        AuthUser(scratch.admin_id),
        Path((scratch.slug.clone(), app_b)),
        Json(api::routes::label::CreateLabel {
            name: "Fresh".to_string(),
            color: None,
            description: None,
            external_source: None,
            external_id: None,
            parent_id: None,
            sort_order: None,
        }),
    )
    .await
    .expect("create label");
    assert_eq!(status, StatusCode::CREATED);
    let id: Uuid = serde_json::from_value(body["id"].clone()).expect("label id");
    let project_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT project_id FROM labels WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .expect("fetch project id");
    assert_eq!(project_id, None);
    scratch.purge(&pool).await;
}

#[tokio::test]
async fn ws_labels_lists_without_project_membership() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let app_a = scratch.insert_project(&pool, "App A", "APPA").await;
    scratch
        .insert_label(&pool, Some(app_a), "Shared", None)
        .await;
    api::routes::taxonomy_backfill::backfill_labels(&pool, scratch.workspace_id)
        .await
        .expect("backfill labels");

    // Guest is a workspace member with NO project membership: still sees labels.
    let (status, Json(body)) = api::routes::label::ws_labels(
        State(st.clone()),
        AuthUser(scratch.guest_id),
        Path(scratch.slug.clone()),
    )
    .await
    .expect("ws labels");
    assert_eq!(status, StatusCode::OK);
    assert!(body.as_array().unwrap().iter().any(|l| l["name"] == "Shared"));
    scratch.purge(&pool).await;
}

impl Scratch {
    async fn insert_issue_type(&self, pool: &PgPool, name: &str, link_to: Option<Uuid>) -> Uuid {
        let type_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO issue_types (id, workspace_id, name, description, logo_props, is_epic, \
             requires_service, is_default, is_active, level, created_by_id, updated_by_id, \
             created_at, updated_at) \
             VALUES ($1, $2, $3, '', '{}', false, false, false, true, 0, $4, $4, now(), now())",
        )
        .bind(type_id)
        .bind(self.workspace_id)
        .bind(name)
        .bind(self.admin_id)
        .execute(pool)
        .await
        .expect("scratch issue type");
        if let Some(project_id) = link_to {
            sqlx::query(
                "INSERT INTO project_issue_types (id, issue_type_id, project_id, workspace_id, \
                 created_by_id, updated_by_id, created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, $4, now(), now())",
            )
            .bind(type_id)
            .bind(project_id)
            .bind(self.workspace_id)
            .bind(self.admin_id)
            .execute(pool)
            .await
            .expect("scratch project type link");
        }
        type_id
    }

    async fn add_project_member(&self, pool: &PgPool, project_id: Uuid, user_id: Uuid, role: i16) {
        sqlx::query(
            "INSERT INTO project_members (id, workspace_id, project_id, member_id, role, \
             is_active, created_by_id, updated_by_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, true, $5, $5, now(), now())",
        )
        .bind(self.workspace_id)
        .bind(project_id)
        .bind(user_id)
        .bind(role)
        .bind(self.admin_id)
        .execute(pool)
        .await
        .expect("scratch project member");
    }
}

#[tokio::test]
async fn list_project_returns_workspace_types() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let app_a = scratch.insert_project(&pool, "App A", "APPA").await;
    let app_b = scratch.insert_project(&pool, "App B", "APPB").await;
    // Incident linked only to A; Request linked nowhere.
    scratch
        .insert_issue_type(&pool, "Incident", Some(app_a))
        .await;
    scratch.insert_issue_type(&pool, "Request", None).await;
    scratch
        .add_project_member(&pool, app_b, scratch.admin_id, 20)
        .await;

    // Project B (no direct links) still sees the whole workspace set.
    let (status, Json(body)) = api::routes::v1::work_item_type::list_project(
        State(st.clone()),
        AuthUser(scratch.admin_id),
        Path((scratch.slug.clone(), app_b)),
        axum::extract::Query(api::routes::v1::common::PageParams {
            cursor: None,
            per_page: None,
        }),
    )
    .await
    .expect("list project types");
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = body
        .as_array()
        .expect("array body")
        .iter()
        .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
        .collect();
    assert!(names.contains(&"Incident"), "missing Incident: {names:?}");
    assert!(names.contains(&"Request"), "missing Request: {names:?}");
    scratch.purge(&pool).await;
}

impl Scratch {
    #[allow(clippy::too_many_arguments)]
    async fn insert_estimate(
        &self,
        pool: &PgPool,
        project_id: Option<Uuid>,
        name: &str,
        estimate_type: &str,
        points: &[(i32, &str)],
        last_used: bool,
    ) -> (Uuid, Vec<Uuid>) {
        let estimate_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO estimates (id, workspace_id, name, description, type, last_used, \
             project_id, created_by_id, updated_by_id, created_at, updated_at) \
             VALUES ($1, $2, $3, '', $4, $5, $6, $7, $7, now(), now())",
        )
        .bind(estimate_id)
        .bind(self.workspace_id)
        .bind(name)
        .bind(estimate_type)
        .bind(last_used)
        .bind(project_id)
        .bind(self.admin_id)
        .execute(pool)
        .await
        .expect("scratch estimate");
        let mut point_ids = Vec::new();
        for (key, value) in points {
            let point_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO estimate_points (id, estimate_id, key, value, description, \
                 project_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at) \
                 VALUES ($1, $2, $3, $4, '', $5, $6, $7, $7, now(), now())",
            )
            .bind(point_id)
            .bind(estimate_id)
            .bind(key)
            .bind(value)
            .bind(project_id)
            .bind(self.workspace_id)
            .bind(self.admin_id)
            .execute(pool)
            .await
            .expect("scratch estimate point");
            point_ids.push(point_id);
        }
        (estimate_id, point_ids)
    }

    async fn point_issue_at(&self, pool: &PgPool, issue_id: Uuid, point_id: Uuid) {
        sqlx::query("UPDATE issues SET estimate_point_id = $1, updated_at = now() WHERE id = $2")
            .bind(point_id)
            .bind(issue_id)
            .execute(pool)
            .await
            .expect("point issue at estimate point");
    }
}

#[tokio::test]
async fn estimates_merge_unions_points() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let _ = st;

    let app_a = scratch.insert_project(&pool, "App A", "APPA").await;
    let app_b = scratch.insert_project(&pool, "App B", "APPB").await;
    let (_ea, _pa) = scratch
        .insert_estimate(&pool, Some(app_a), "Points", "points", &[(1, "1"), (2, "2"), (3, "3")], false)
        .await;
    let (_eb, pb) = scratch
        .insert_estimate(&pool, Some(app_b), "points", "points", &[(1, "1"), (2, "2"), (5, "5"), (8, "8")], false)
        .await;
    // Issue on B's 5-point (value "5" exists only in B).
    let issue_id = scratch.insert_issue(&pool, app_b, "sized ticket").await;
    let five_point = pb[2];
    scratch.point_issue_at(&pool, issue_id, five_point).await;

    api::routes::taxonomy_backfill::backfill_estimates(&pool, scratch.workspace_id)
        .await
        .expect("backfill estimates");

    // One surviving estimate; union of points {1,2,3,5,8}.
    let names: Vec<String> =
        sqlx::query_scalar("SELECT name FROM estimates WHERE workspace_id = $1 AND deleted_at IS NULL")
            .bind(scratch.workspace_id)
            .fetch_all(&pool)
            .await
            .expect("list estimates");
    assert_eq!(names, vec!["Points".to_string()]);
    let mut values: Vec<String> = sqlx::query_scalar(
        "SELECT ep.value FROM estimate_points ep JOIN estimates e ON e.id = ep.estimate_id \
         WHERE e.workspace_id = $1 AND ep.deleted_at IS NULL ORDER BY ep.value",
    )
    .bind(scratch.workspace_id)
    .fetch_all(&pool)
    .await
    .expect("list points");
    values.sort();
    assert_eq!(values, vec!["1", "2", "3", "5", "8"]);
    // Issue repointed (same value, canonical estimate).
    let (point_est, point_val): (Uuid, String) = sqlx::query_as(
        "SELECT e.id, ep.value FROM issues i JOIN estimate_points ep ON ep.id = i.estimate_point_id \
         JOIN estimates e ON e.id = ep.estimate_id WHERE i.id = $1",
    )
    .bind(issue_id)
    .fetch_one(&pool)
    .await
    .expect("issue point");
    assert_eq!(point_val, "5");
    let canonical: Uuid = sqlx::query_scalar(
        "SELECT id FROM estimates WHERE workspace_id = $1 AND deleted_at IS NULL",
    )
    .bind(scratch.workspace_id)
    .fetch_one(&pool)
    .await
    .expect("canonical estimate");
    assert_eq!(point_est, canonical);
    // Exactly one last_used workspace-wide; settings pointer follows.
    let actives: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM estimates WHERE workspace_id = $1 AND deleted_at IS NULL AND last_used = true",
    )
    .bind(scratch.workspace_id)
    .fetch_one(&pool)
    .await
    .expect("count active");
    assert_eq!(actives, 1);
    let pointed: Option<Uuid> = sqlx::query_scalar(
        "SELECT estimate_id FROM workspace_settings WHERE workspace_id = $1",
    )
    .bind(scratch.workspace_id)
    .fetch_one(&pool)
    .await
    .expect("settings pointer");
    assert_eq!(pointed, Some(canonical));
    scratch.purge(&pool).await;
}

#[tokio::test]
async fn project_estimate_endpoints_serve_workspace_set() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let app_a = scratch.insert_project(&pool, "App A", "APPA").await;
    let app_b = scratch.insert_project(&pool, "App B", "APPB").await;
    scratch
        .insert_estimate(&pool, Some(app_a), "T-Shirt", "categories", &[(1, "XS"), (2, "S")], true)
        .await;
    api::routes::taxonomy_backfill::backfill_estimates(&pool, scratch.workspace_id)
        .await
        .expect("backfill estimates");

    // Project B sees the workspace set (incl. A's estimate).
    let (status, Json(body)) = api::routes::estimate::list(
        State(st.clone()),
        AuthUser(scratch.admin_id),
        Path((scratch.slug.clone(), app_b)),
    )
    .await
    .expect("list estimates");
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = body
        .as_array()
        .expect("array body")
        .iter()
        .filter_map(|e| e.get("name").and_then(|n| n.as_str()))
        .collect();
    assert!(names.contains(&"T-Shirt"), "missing T-Shirt: {names:?}");
    scratch.purge(&pool).await;
}
