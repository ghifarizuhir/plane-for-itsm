//! RCB/TCB backend integration tests. DB-backed; run serially:
//! `DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
//!  cargo test -p api --test release_review_test -- --test-threads=1`

use api::middleware::auth::AuthUser;
use api::routes::release::{
    create, destroy, detail, list, patch, CreateRelease, ListParams, PatchRelease,
};
use api::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use common::config::AppConfig;
use serde_json::Value;
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

struct Scratch {
    slug: String,
    workspace_id: Uuid,
    user_id: Uuid,
    project_id: Uuid,
    state_id: Uuid,
    type_id: Uuid,
    extra_users: Vec<Uuid>,
}

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let slug = format!("rr-{}", Uuid::new_v4().simple());
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let state_id = Uuid::new_v4();
        let type_id = Uuid::new_v4();
        let identifier = format!("RR{}", &Uuid::new_v4().simple().to_string()[..8]).to_uppercase();

        insert_user(pool, user_id, &slug).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'Release Scratch', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
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
             VALUES ($1, now(), now(), 'Release Scratch', '', 2, $2, $3, false, false, false, \
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
             VALUES ($1, 'New', '', '#F59E0B', 'new', $2, $3, 65535, \
             'backlog', true, false, now(), now())",
        )
        .bind(state_id)
        .bind(project_id)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch state");
        sqlx::query(
            "INSERT INTO issue_types (id, name, description, logo_props, workspace_id, is_active, \
             is_default, level, is_epic, created_at, updated_at) \
             VALUES ($1, 'Change', '', '{}'::jsonb, $2, true, true, 0, false, now(), now())",
        )
        .bind(type_id)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch issue type");

        Self {
            slug,
            workspace_id,
            user_id,
            project_id,
            state_id,
            type_id,
            extra_users: Vec::new(),
        }
    }

    async fn add_actor(&mut self, pool: &PgPool, ws_role: Option<i16>) -> Uuid {
        let user_id = Uuid::new_v4();
        let username = format!("{}-{}", self.slug, &user_id.simple().to_string()[..8]);
        insert_user(pool, user_id, &username).await;
        if let Some(role) = ws_role {
            insert_workspace_member(pool, user_id, self.workspace_id, role).await;
        }
        self.extra_users.push(user_id);
        user_id
    }

    async fn insert_issue(&self, pool: &PgPool) -> Uuid {
        let mut tx = pool.begin().await.expect("scratch tx");
        let (issue_id, sequence_id): (Uuid, i32) = sqlx::query_as(
            "INSERT INTO issues (id, name, description_html, description_json, priority, is_draft, \
             sort_order, sequence_id, state_id, type_id, project_id, workspace_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), 'Change request', '<p></p>', '{}', 'none', false, \
             65535, (SELECT COALESCE(MAX(sequence_id), 0) + 1 FROM issues WHERE project_id = $3), \
             $1, $2, $3, $4, now(), now()) RETURNING id, sequence_id",
        )
        .bind(self.state_id)
        .bind(self.type_id)
        .bind(self.project_id)
        .bind(self.workspace_id)
        .fetch_one(&mut *tx)
        .await
        .expect("scratch issue");
        // DB-wide `issue_sequences` invariant (checked by a parallel test binary).
        sqlx::query(
            "INSERT INTO issue_sequences (id, sequence, issue_id, project_id, workspace_id, \
             created_by_id, deleted, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, false, now(), now())",
        )
        .bind(sequence_id)
        .bind(issue_id)
        .bind(self.project_id)
        .bind(self.workspace_id)
        .bind(self.user_id)
        .execute(&mut *tx)
        .await
        .expect("scratch issue sequence");
        tx.commit().await.expect("scratch commit");
        issue_id
    }

    async fn cleanup(&self, pool: &PgPool) {
        sqlx::query("DELETE FROM notifications WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM issue_sequences WHERE project_id = $1")
            .bind(self.project_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        let mut user_ids = vec![self.user_id];
        user_ids.extend(self.extra_users.iter().copied());
        sqlx::query("DELETE FROM users WHERE id = ANY($1)")
            .bind(&user_ids)
            .execute(pool)
            .await
            .ok();
    }
}

fn create_body(name: &str) -> CreateRelease {
    CreateRelease {
        name: Some(name.to_string()),
        version: None,
        description_html: None,
        status: None,
        target_date: None,
    }
}

#[tokio::test]
async fn create_release_defaults_and_sequence() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body("Rilis 2026.10")),
    )
    .await
    .expect("create release");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["sequence_id"], 1);
    assert_eq!(body["name"], "Rilis 2026.10");
    assert_eq!(body["status"], "draft");
    assert!(body["version"].is_null());
    assert!(body["target_date"].is_null());

    let (status, Json(second)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body("Rilis 2026.11")),
    )
    .await
    .expect("create second release");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(second["sequence_id"], 2);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_release_validates_name_status_and_membership() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;

    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body("   ")),
    )
    .await
    .expect("blank name");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "name is required");

    let mut bad_status = create_body("Rilis X");
    bad_status.status = Some("bogus".into());
    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(bad_status),
    )
    .await
    .expect("bad status");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid status");

    let outsider = scratch.add_actor(&st.pool, None).await;
    let (status, _) = create(
        State(st.clone()),
        AuthUser(outsider),
        Path(scratch.slug.clone()),
        Json(create_body("Rilis Y")),
    )
    .await
    .expect("outsider");
    assert_eq!(status, StatusCode::FORBIDDEN);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn list_releases_filters_status() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body("Rilis A")),
    )
    .await
    .expect("create first");
    let mut cancelled_body = create_body("Rilis B");
    cancelled_body.status = Some("cancelled".into());
    create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(cancelled_body),
    )
    .await
    .expect("create second");

    let (status, Json(all)) = list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Query(ListParams {
            status: None,
            target_date_from: None,
            target_date_to: None,
        }),
    )
    .await
    .expect("list all");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all.as_array().unwrap().len(), 2);

    let (_, Json(cancelled)) = list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Query(ListParams {
            status: Some("cancelled".into()),
            target_date_from: None,
            target_date_to: None,
        }),
    )
    .await
    .expect("list filtered");
    assert_eq!(cancelled.as_array().unwrap().len(), 1);
    assert_eq!(cancelled[0]["name"], "Rilis B");

    scratch.cleanup(&st.pool).await;
}

async fn insert_review_request(
    pool: &PgPool,
    workspace_id: Uuid,
    release_id: Uuid,
    user_id: Uuid,
    status: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO review_requests (id, workspace_id, board_type, release_id, status, \
         submission_note, submitted_by_id, submitted_at, created_at, updated_at, \
         created_by_id, updated_by_id) \
         VALUES (gen_random_uuid(), $1, 'rcb', $2, $3, '', $4, now(), now(), now(), $4, $4) \
         RETURNING id",
    )
    .bind(workspace_id)
    .bind(release_id)
    .bind(status)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .expect("scratch review request")
}

fn release_id(body: &Value) -> Uuid {
    body["id"].as_str().unwrap().parse().unwrap()
}

#[tokio::test]
async fn detail_release_includes_empty_bundles() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    let (_, Json(release)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body("Rilis detail")),
    )
    .await
    .expect("create release");
    let rid = release_id(&release);

    let (status, Json(body)) = detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid)),
    )
    .await
    .expect("detail");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "Rilis detail");
    assert_eq!(body["changes"].as_array().unwrap().len(), 0);
    assert_eq!(body["review_requests"].as_array().unwrap().len(), 0);

    let (status, _) = detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), Uuid::new_v4())),
    )
    .await
    .expect("missing detail");
    assert_eq!(status, StatusCode::NOT_FOUND);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn patch_release_updates_and_clears_optional_fields() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    let (_, Json(release)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(CreateRelease {
            name: Some("Rilis patch".into()),
            version: Some("v1.0.0".into()),
            description_html: None,
            status: None,
            target_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()),
        }),
    )
    .await
    .expect("create release");
    let rid = release_id(&release);
    assert_eq!(release["version"], "v1.0.0");

    let (status, Json(body)) = patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid)),
        Json(PatchRelease {
            name: Some("Rilis patch v2".into()),
            version: Some(None),
            description_html: None,
            status: Some("planned".into()),
            target_date: Some(Some(
                chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
            )),
        }),
    )
    .await
    .expect("patch");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "Rilis patch v2");
    assert!(body["version"].is_null());
    assert_eq!(body["status"], "planned");
    assert_eq!(body["target_date"], "2026-11-01");

    let (status, Json(body)) = patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid)),
        Json(PatchRelease {
            status: Some("bogus".into()),
            ..Default::default()
        }),
    )
    .await
    .expect("bad status");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid status");

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn destroy_release_guards_active_review_and_permissions() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;

    let (_, Json(release)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(create_body("Rilis hapus")),
    )
    .await
    .expect("create release");
    let rid = release_id(&release);

    let member = scratch.add_actor(&st.pool, Some(15)).await;
    let (status, _) = destroy(
        State(st.clone()),
        AuthUser(member),
        Path((scratch.slug.clone(), rid)),
    )
    .await
    .expect("non-creator delete");
    assert_eq!(status, StatusCode::FORBIDDEN);

    let request_id = insert_review_request(
        &st.pool,
        scratch.workspace_id,
        rid,
        scratch.user_id,
        "pending",
    )
    .await;
    let (status, Json(body)) = destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid)),
    )
    .await
    .expect("guarded delete");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Release has an active review request");

    sqlx::query("UPDATE review_requests SET status = 'withdrawn' WHERE id = $1")
        .bind(request_id)
        .execute(&st.pool)
        .await
        .expect("withdraw request");

    let (status, _) = destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), rid)),
    )
    .await
    .expect("delete");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, Json(all)) = list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Query(ListParams {
            status: None,
            target_date_from: None,
            target_date_to: None,
        }),
    )
    .await
    .expect("list after delete");
    assert_eq!(all.as_array().unwrap().len(), 0);

    scratch.cleanup(&st.pool).await;
}
