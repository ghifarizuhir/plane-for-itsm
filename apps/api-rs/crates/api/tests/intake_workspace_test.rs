//! Workspace intake queue (`GET /api/workspaces/:slug/intake-issues/`).
//! Run with `--test-threads=1`.

use api::middleware::auth::AuthUser;
use api::routes::intake::{workspace_intake_issues, WsIntakeIssuesQuery};
use api::state::AppState;
use axum::extract::{Path, Query, State};
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

struct Scratch {
    slug: String,
    workspace_id: Uuid,
    admin_id: Uuid,
    bob_id: Uuid,
    carol_id: Uuid,
    dave_id: Uuid,
    mallory_id: Uuid,
    outsider_id: Uuid,
    p1: Uuid,
    p2: Uuid,
    p3: Uuid,
}

async fn insert_user(pool: &PgPool, tag: &str, slug: &str) -> Uuid {
    let user_id = Uuid::new_v4();
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
    .bind(format!("{slug}-{tag}"))
    .bind(format!("{slug}-{tag}@example.invalid"))
    .bind(Uuid::new_v4().simple().to_string())
    .execute(pool)
    .await
    .expect("scratch user");
    user_id
}

async fn insert_ws_member(pool: &PgPool, user_id: Uuid, workspace_id: Uuid) {
    sqlx::query(
        "INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, \
         workspace_id, view_props, default_props, issue_props, explored_features, \
         getting_started_checklist, tips, is_active) \
         VALUES (gen_random_uuid(), now(), now(), 15, $1, $2, '{}', '{}', '{}', '{}', '{}', \
         '{}', true)",
    )
    .bind(user_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch workspace member");
}

async fn insert_project(pool: &PgPool, workspace_id: Uuid, slug: &str, tag: &str, guest_view_all: bool) -> Uuid {
    let project_id = Uuid::new_v4();
    let identifier = format!("ITW{}", &Uuid::new_v4().simple().to_string()[..8]).to_uppercase();
    sqlx::query(
        "INSERT INTO projects (id, created_at, updated_at, name, description, network, \
         identifier, workspace_id, cycle_view, module_view, issue_views_view, page_view, \
         intake_view, archive_in, close_in, logo_props, is_time_tracking_enabled, \
         is_issue_type_enabled, guest_view_all_features, timezone) \
         VALUES ($1, now(), now(), $2, '', 2, $3, $4, false, false, false, false, \
         false, 30, 30, '{}'::jsonb, false, false, $5, 'UTC')",
    )
    .bind(project_id)
    .bind(format!("{slug}-{tag}"))
    .bind(&identifier)
    .bind(workspace_id)
    .bind(guest_view_all)
    .execute(pool)
    .await
    .expect("scratch project");
    project_id
}

async fn insert_member(pool: &PgPool, user_id: Uuid, project_id: Uuid, workspace_id: Uuid, role: i16) {
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

async fn insert_intake(pool: &PgPool, name: &str, project_id: Uuid, workspace_id: Uuid) -> Uuid {
    let intake_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO intakes (id, name, description, is_default, view_props, logo_props, \
         project_id, workspace_id, created_at, updated_at) \
         VALUES ($1, $2, '', false, '{}'::jsonb, '{}'::jsonb, $3, $4, now(), now())",
    )
    .bind(intake_id)
    .bind(name)
    .bind(project_id)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch intake");
    intake_id
}

#[allow(clippy::too_many_arguments)]
async fn insert_issue(
    pool: &PgPool,
    name: &str,
    seq: i64,
    project_id: Uuid,
    workspace_id: Uuid,
    created_by: Option<Uuid>,
    age_hours: i64,
) -> Uuid {
    let issue_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, name, description_html, description_json, description_stripped, \
         priority, is_draft, sort_order, sequence_id, state_id, type_id, project_id, workspace_id, \
         created_at, updated_at, created_by_id) \
         VALUES ($1, $2, '<p></p>', '{}', '', 'none', false, 65535, $3, NULL, NULL, $4, $5, \
         now() - ($6 || ' hours')::interval, now(), $7)",
    )
    .bind(issue_id)
    .bind(name)
    .bind(seq)
    .bind(project_id)
    .bind(workspace_id)
    .bind(age_hours)
    .bind(created_by)
    .execute(pool)
    .await
    .expect("scratch issue");
    issue_id
}

#[allow(clippy::too_many_arguments)]
async fn link_intake(
    pool: &PgPool,
    intake_id: Uuid,
    issue_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    status: i32,
    snoozed: bool,
    created_by: Option<Uuid>,
) {
    sqlx::query(
        "INSERT INTO intake_issues (id, intake_id, issue_id, project_id, workspace_id, status, \
         snoozed_till, source, duplicate_to_id, created_by_id, updated_by_id, extra, \
         deleted_at, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, \
         CASE WHEN $6 THEN now() + interval '1 day' ELSE NULL END, 'IN_APP', NULL, $7, NULL, \
         '{}'::jsonb, NULL, now(), now())",
    )
    .bind(intake_id)
    .bind(issue_id)
    .bind(project_id)
    .bind(workspace_id)
    .bind(status)
    .bind(snoozed)
    .bind(created_by)
    .execute(pool)
    .await
    .expect("scratch intake issue");
}

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let slug = format!("itw-{}", Uuid::new_v4().simple());
        let workspace_id = Uuid::new_v4();
        let admin_id = insert_user(pool, "admin", &slug).await;
        let bob_id = insert_user(pool, "bob", &slug).await;
        let carol_id = insert_user(pool, "carol", &slug).await;
        let dave_id = insert_user(pool, "dave", &slug).await;
        let mallory_id = insert_user(pool, "mallory", &slug).await;
        let outsider_id = Uuid::new_v4();

        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'Intake Workspace Scratch', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
        )
        .bind(workspace_id)
        .bind(&slug)
        .bind(admin_id)
        .execute(pool)
        .await
        .expect("scratch workspace");

        for uid in [admin_id, bob_id, carol_id, dave_id, mallory_id] {
            insert_ws_member(pool, uid, workspace_id).await;
        }

        let p1 = insert_project(pool, workspace_id, &slug, "one", true).await;
        let p2 = insert_project(pool, workspace_id, &slug, "two", false).await;
        let p3 = insert_project(pool, workspace_id, &slug, "three", false).await;

        insert_member(pool, admin_id, p1, workspace_id, 20).await;
        insert_member(pool, admin_id, p2, workspace_id, 20).await;
        insert_member(pool, bob_id, p1, workspace_id, 15).await;
        insert_member(pool, carol_id, p2, workspace_id, 5).await;
        insert_member(pool, dave_id, p1, workspace_id, 5).await;

        let intake_alpha = insert_intake(pool, "Alpha Intake", p1, workspace_id).await;
        let intake_zulu = insert_intake(pool, "Zulu Intake", p1, workspace_id).await;
        let intake_beta = insert_intake(pool, "Beta Intake", p2, workspace_id).await;
        let intake_gamma = insert_intake(pool, "Gamma Intake", p3, workspace_id).await;

        // p1 / Alpha: pending x2 (admin, bob), snoozed, accepted, rejected, duplicate.
        let i1 = insert_issue(pool, "Fix login bug", 1, p1, workspace_id, Some(admin_id), 0).await;
        link_intake(pool, intake_alpha, i1, p1, workspace_id, -2, false, Some(admin_id)).await;
        // Same issue bridged through the second intake (multi-intake union + duplicate-row case).
        link_intake(pool, intake_zulu, i1, p1, workspace_id, -2, false, Some(admin_id)).await;
        let i2 = insert_issue(pool, "Update docs", 2, p1, workspace_id, Some(bob_id), 1).await;
        link_intake(pool, intake_alpha, i2, p1, workspace_id, -2, false, Some(bob_id)).await;
        let i3 = insert_issue(pool, "Snoozed follow-up", 3, p1, workspace_id, Some(admin_id), 5).await;
        link_intake(pool, intake_alpha, i3, p1, workspace_id, 0, true, Some(admin_id)).await;
        let i4 = insert_issue(pool, "Accepted feature", 4, p1, workspace_id, Some(admin_id), 6).await;
        link_intake(pool, intake_alpha, i4, p1, workspace_id, 1, false, Some(admin_id)).await;
        let i5 = insert_issue(pool, "Rejected spam", 5, p1, workspace_id, Some(admin_id), 7).await;
        link_intake(pool, intake_alpha, i5, p1, workspace_id, -1, false, Some(admin_id)).await;
        let i6 = insert_issue(pool, "Duplicate ticket", 6, p1, workspace_id, Some(admin_id), 8).await;
        link_intake(pool, intake_alpha, i6, p1, workspace_id, 2, false, Some(admin_id)).await;
        // p1 / Zulu only.
        let i7 = insert_issue(pool, "Zulu request", 7, p1, workspace_id, Some(admin_id), 2).await;
        link_intake(pool, intake_zulu, i7, p1, workspace_id, -2, false, Some(admin_id)).await;
        // p2: pending (admin), pending (carol), accepted (admin).
        let i8 = insert_issue(pool, "Beta request", 1, p2, workspace_id, Some(admin_id), 3).await;
        link_intake(pool, intake_beta, i8, p2, workspace_id, -2, false, Some(admin_id)).await;
        let i9 = insert_issue(pool, "Carol personal", 2, p2, workspace_id, Some(carol_id), 4).await;
        link_intake(pool, intake_beta, i9, p2, workspace_id, -2, false, Some(carol_id)).await;
        let i10 = insert_issue(pool, "Beta accepted", 3, p2, workspace_id, Some(admin_id), 9).await;
        link_intake(pool, intake_beta, i10, p2, workspace_id, 1, false, Some(admin_id)).await;
        // p3 (no memberships): must never appear.
        let i11 = insert_issue(pool, "P3 hidden", 1, p3, workspace_id, Some(admin_id), 10).await;
        link_intake(pool, intake_gamma, i11, p3, workspace_id, -2, false, Some(admin_id)).await;

        Self {
            slug,
            workspace_id,
            admin_id,
            bob_id,
            carol_id,
            dave_id,
            mallory_id,
            outsider_id,
            p1,
            p2,
            p3,
        }
    }

    async fn cleanup(&self, pool: &PgPool) {
        sqlx::query("DELETE FROM intake_issues WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM issues WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM intakes WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        for pid in [self.p1, self.p2, self.p3] {
            sqlx::query("DELETE FROM project_members WHERE project_id = $1")
                .bind(pid)
                .execute(pool)
                .await
                .ok();
            sqlx::query("DELETE FROM projects WHERE id = $1")
                .bind(pid)
                .execute(pool)
                .await
                .ok();
        }
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
        for uid in [
            self.admin_id,
            self.bob_id,
            self.carol_id,
            self.dave_id,
            self.mallory_id,
        ] {
            sqlx::query("DELETE FROM users WHERE id = $1")
                .bind(uid)
                .execute(pool)
                .await
                .ok();
        }
    }
}

fn names(body: &serde_json::Value) -> Vec<String> {
    body["results"]
        .as_array()
        .expect("envelope has results")
        .iter()
        .map(|row| row["issue"]["name"].as_str().unwrap_or("").to_string())
        .collect()
}

fn statuses(body: &serde_json::Value) -> Vec<i64> {
    body["results"]
        .as_array()
        .expect("envelope has results")
        .iter()
        .map(|row| row["status"].as_i64().unwrap_or(999))
        .collect()
}

async fn call(
    st: &AppState,
    user: Uuid,
    slug: &str,
    q: WsIntakeIssuesQuery,
) -> (StatusCode, serde_json::Value) {
    let (status, Json(body)) = workspace_intake_issues(State(st.clone()), AuthUser(user), Path(slug.to_string()), Query(q))
        .await
        .unwrap();
    (status, body)
}

fn open_all() -> WsIntakeIssuesQuery {
    WsIntakeIssuesQuery {
        type_: None,
        project_id: None,
        search: None,
        per_page: None,
        cursor: None,
    }
}

fn with_type(t: &str) -> WsIntakeIssuesQuery {
    WsIntakeIssuesQuery {
        type_: Some(t.to_string()),
        ..Default::default()
    }
}

#[tokio::test]
async fn non_workspace_member_gets_403() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, _) = call(&st, scratch.outsider_id, &scratch.slug, open_all()).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn non_project_member_gets_empty_list() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(&st, scratch.mallory_id, &scratch.slug, open_all()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["results"].as_array().unwrap().len(), 0);
    assert_eq!(body["total_results"], 0);
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn open_tab_groups_pending_and_snoozed_newest_first() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(&st, scratch.admin_id, &scratch.slug, open_all()).await;
    assert_eq!(status, StatusCode::OK);
    // Newest first: Fix login bug (0h), Update docs (1h), Zulu request (2h),
    // Beta request (3h), Carol personal (4h), Snoozed follow-up (5h).
    assert_eq!(
        names(&body),
        vec![
            "Fix login bug".to_string(),
            "Fix login bug".to_string(),
            "Update docs".to_string(),
            "Zulu request".to_string(),
            "Beta request".to_string(),
            "Carol personal".to_string(),
            "Snoozed follow-up".to_string(),
        ]
    );
    assert!(statuses(&body).iter().all(|s| *s == -2 || *s == 0));
    assert_eq!(body["total_results"], 7);
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn closed_tab_lists_accepted_rejected_duplicate() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(&st, scratch.admin_id, &scratch.slug, with_type("closed")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        names(&body),
        vec![
            "Accepted feature".to_string(),
            "Rejected spam".to_string(),
            "Duplicate ticket".to_string(),
            "Beta accepted".to_string(),
        ]
    );
    assert_eq!(body["total_results"], 4);
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn invalid_type_falls_back_to_open() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (_, open_body) = call(&st, scratch.admin_id, &scratch.slug, open_all()).await;
    let (status, invalid_body) = call(&st, scratch.admin_id, &scratch.slug, with_type("nope")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(invalid_body["results"], open_body["results"]);
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn project_filter_scopes_and_still_enforces_visibility() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(
        &st,
        scratch.admin_id,
        &scratch.slug,
        WsIntakeIssuesQuery {
            project_id: Some(scratch.p2),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // Admin sees both p2 rows (Beta request + Carol personal on the open tab).
    assert_eq!(names(&body), vec!["Beta request".to_string(), "Carol personal".to_string()]);

    let (status, body) = call(
        &st,
        scratch.carol_id,
        &scratch.slug,
        WsIntakeIssuesQuery {
            project_id: Some(scratch.p1),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["results"].as_array().unwrap().len(), 0);
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn restricted_guest_sees_only_own_requests() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    // Carol is a restricted guest (role 5, guest_view_all_features=false) on p2.
    let (status, body) = call(&st, scratch.carol_id, &scratch.slug, open_all()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(names(&body), vec!["Carol personal".to_string()]);

    // Closed tab: Carol owns no closed rows; the other p2 closed row stays hidden.
    let (status, body) = call(&st, scratch.carol_id, &scratch.slug, with_type("closed")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["results"].as_array().unwrap().len(), 0);
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn open_guest_sees_public_project_requests() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    // Dave is a guest (role 5) on the open p1 project: sees everything there.
    let (status, body) = call(&st, scratch.dave_id, &scratch.slug, open_all()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        names(&body),
        vec![
            "Fix login bug".to_string(),
            "Fix login bug".to_string(),
            "Update docs".to_string(),
            "Zulu request".to_string(),
            "Snoozed follow-up".to_string(),
        ]
    );
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn search_filters_by_issue_name() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(
        &st,
        scratch.admin_id,
        &scratch.slug,
        WsIntakeIssuesQuery {
            search: Some("LOGIN".to_string()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // Both links of the bridged issue match.
    assert_eq!(body["total_results"], 2);
    assert!(names(&body).iter().all(|n| n == "Fix login bug"));
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn pagination_cursors_page_results_without_overlap() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, first) = call(
        &st,
        scratch.admin_id,
        &scratch.slug,
        WsIntakeIssuesQuery {
            per_page: Some("2".to_string()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["total_results"], 7);
    assert_eq!(first["next_page_results"], true);
    let first_ids: Vec<String> = first["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(first_ids.len(), 2);
    let cursor = first["next_cursor"].as_str().unwrap().to_string();

    let (status, second) = call(
        &st,
        scratch.admin_id,
        &scratch.slug,
        WsIntakeIssuesQuery {
            per_page: Some("2".to_string()),
            cursor: Some(cursor),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let second_ids: Vec<String> = second["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(second_ids.len(), 2);
    for id in &second_ids {
        assert!(!first_ids.contains(id));
    }
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn multi_intake_union_lists_both_intakes() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(
        &st,
        scratch.admin_id,
        &scratch.slug,
        WsIntakeIssuesQuery {
            project_id: Some(scratch.p1),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // Zulu-only row is present although the project endpoint only serves the
    // first intake by name.
    assert!(names(&body).contains(&"Zulu request".to_string()));
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn same_issue_bridged_twice_appears_twice() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(&st, scratch.admin_id, &scratch.slug, open_all()).await;
    assert_eq!(status, StatusCode::OK);
    let dup_ids: Vec<String> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["issue"]["name"].as_str() == Some("Fix login bug"))
        .map(|row| row["id"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(dup_ids.len(), 2);
    assert_ne!(dup_ids[0], dup_ids[1]);
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn row_shape_carries_issue_nesting_and_project() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(&st, scratch.admin_id, &scratch.slug, open_all()).await;
    assert_eq!(status, StatusCode::OK);
    let row = &body["results"].as_array().unwrap()[0];
    assert!(row["id"].is_string());
    assert!(row["status"].is_number());
    assert_eq!(row["issue"]["project_id"].as_str(), Some(scratch.p1.to_string()).as_deref());
    assert!(row["issue"]["sequence_id"].is_number());
    assert!(row["issue"]["name"].as_str().is_some());
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn invalid_cursor_is_a_400() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, _) = call(
        &st,
        scratch.admin_id,
        &scratch.slug,
        WsIntakeIssuesQuery {
            cursor: Some("not-a-cursor".to_string()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    scratch.cleanup(&pool).await;
}
