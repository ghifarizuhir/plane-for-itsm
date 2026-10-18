//! Workspace pages endpoint (`GET /api/workspaces/:slug/pages/`).
//! Run with `--test-threads=1`.

use api::middleware::auth::AuthUser;
use api::routes::page::{workspace_pages, WsPagesQuery};
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

async fn make_user(pool: &PgPool, tag: &str, slug: &str) -> Uuid {
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

async fn make_ws_member(pool: &PgPool, user_id: Uuid, workspace_id: Uuid, role: i16) {
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

async fn make_project(pool: &PgPool, workspace_id: Uuid, slug: &str, tag: &str, guest_view_all: bool) -> Uuid {
    let project_id = Uuid::new_v4();
    let identifier = format!("KBW{}", &Uuid::new_v4().simple().to_string()[..8]).to_uppercase();
    sqlx::query(
        "INSERT INTO projects (id, created_at, updated_at, name, description, network, \
         identifier, workspace_id, cycle_view, module_view, issue_views_view, page_view, \
         intake_view, archive_in, close_in, logo_props, is_time_tracking_enabled, \
         is_issue_type_enabled, guest_view_all_features, timezone) \
         VALUES ($1, now(), now(), $2, '', 2, $3, $4, false, false, false, false, \
         false, 30, 30, '{}'::jsonb, false, false, $5, 'UTC')",
    )
    .bind(project_id)
    .bind(format!("{slug}-project-{tag}"))
    .bind(&identifier)
    .bind(workspace_id)
    .bind(guest_view_all)
    .execute(pool)
    .await
    .expect("scratch project");
    project_id
}

async fn make_member(pool: &PgPool, user_id: Uuid, project_id: Uuid, workspace_id: Uuid, role: i16) {
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

#[allow(clippy::too_many_arguments)]
async fn insert_page(
    pool: &PgPool,
    page_id: Uuid,
    name: &str,
    owner_id: Uuid,
    workspace_id: Uuid,
    access: i16,
    archived: bool,
    parent_id: Option<Uuid>,
) {
    sqlx::query(
        "INSERT INTO pages (id, name, description_json, description_binary, description_html, \
         description_stripped, owned_by_id, created_by_id, updated_by_id, workspace_id, color, \
         access, parent_id, archived_at, is_locked, view_props, logo_props, is_global, \
         sort_order, created_at, updated_at) \
         VALUES ($1, $2, '{}'::jsonb, NULL, '<p>x</p>', 'x', $3, $3, $3, $4, '', $5, \
         $6, CASE WHEN $7 THEN CURRENT_DATE ELSE NULL END, false, '{}'::jsonb, '{}'::jsonb, \
         false, 65535, now(), now())",
    )
    .bind(page_id)
    .bind(name)
    .bind(owner_id)
    .bind(workspace_id)
    .bind(access)
    .bind(parent_id)
    .bind(archived)
    .execute(pool)
    .await
    .expect("scratch page");
}

async fn link_page(pool: &PgPool, workspace_id: Uuid, project_id: Uuid, page_id: Uuid, user_id: Uuid) {
    sqlx::query(
        "INSERT INTO project_pages (id, workspace_id, project_id, page_id, created_by_id, \
         updated_by_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $4, now(), now())",
    )
    .bind(workspace_id)
    .bind(project_id)
    .bind(page_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("scratch project page link");
}

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let slug = format!("kbw-{}", Uuid::new_v4().simple());
        let workspace_id = Uuid::new_v4();
        let admin_id = make_user(pool, "admin", &slug).await;
        let bob_id = make_user(pool, "bob", &slug).await;
        let carol_id = make_user(pool, "carol", &slug).await;
        let dave_id = make_user(pool, "dave", &slug).await;
        let mallory_id = make_user(pool, "mallory", &slug).await;
        let outsider_id = Uuid::new_v4();

        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'KB Workspace Scratch', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
        )
        .bind(workspace_id)
        .bind(&slug)
        .bind(admin_id)
        .execute(pool)
        .await
        .expect("scratch workspace");

        for (user, role) in [
            (admin_id, 20),
            (bob_id, 15),
            (carol_id, 15),
            (dave_id, 15),
            (mallory_id, 15),
        ] {
            make_ws_member(pool, user, workspace_id, role).await;
        }

        let p1 = make_project(pool, workspace_id, &slug, "one", true).await;
        let p2 = make_project(pool, workspace_id, &slug, "two", false).await;
        let p3 = make_project(pool, workspace_id, &slug, "three", false).await;

        make_member(pool, admin_id, p1, workspace_id, 20).await;
        make_member(pool, admin_id, p2, workspace_id, 20).await;
        make_member(pool, bob_id, p1, workspace_id, 15).await;
        make_member(pool, carol_id, p2, workspace_id, 5).await;
        make_member(pool, dave_id, p1, workspace_id, 5).await;

        // p1: Alpha Guide (public, admin), Alice Private (private, admin),
        // Bob Private (private, bob), Old Runbook (public archived, admin).
        let alpha_id = Uuid::new_v4();
        insert_page(pool, alpha_id, "Alpha Guide", admin_id, workspace_id, 0, false, None).await;
        link_page(pool, workspace_id, p1, alpha_id, admin_id).await;

        let alice_private_id = Uuid::new_v4();
        insert_page(pool, alice_private_id, "Alice Private", admin_id, workspace_id, 1, false, None).await;
        link_page(pool, workspace_id, p1, alice_private_id, admin_id).await;

        let bob_private_id = Uuid::new_v4();
        insert_page(pool, bob_private_id, "Bob Private", bob_id, workspace_id, 1, false, None).await;
        link_page(pool, workspace_id, p1, bob_private_id, bob_id).await;

        let old_id = Uuid::new_v4();
        insert_page(pool, old_id, "Old Runbook", admin_id, workspace_id, 0, true, None).await;
        link_page(pool, workspace_id, p1, old_id, admin_id).await;

        // p2: Beta Manual (public, admin), Carol Private (private, carol).
        let beta_id = Uuid::new_v4();
        insert_page(pool, beta_id, "Beta Manual", admin_id, workspace_id, 0, false, None).await;
        link_page(pool, workspace_id, p2, beta_id, admin_id).await;

        let carol_private_id = Uuid::new_v4();
        insert_page(pool, carol_private_id, "Carol Private", carol_id, workspace_id, 1, false, None).await;
        link_page(pool, workspace_id, p2, carol_private_id, carol_id).await;

        // Multi-linked public page (p1 + p2 + p3, the last one invisible to admin).
        let multi_id = Uuid::new_v4();
        insert_page(pool, multi_id, "Multi SOP", admin_id, workspace_id, 0, false, None).await;
        link_page(pool, workspace_id, p1, multi_id, admin_id).await;
        link_page(pool, workspace_id, p2, multi_id, admin_id).await;
        link_page(pool, workspace_id, p3, multi_id, admin_id).await;

        // Child of Alpha Guide stays visible as a flat row with parent set.
        let child_id = Uuid::new_v4();
        insert_page(
            pool,
            child_id,
            "Child Note",
            admin_id,
            workspace_id,
            0,
            false,
            Some(alpha_id),
        )
        .await;
        link_page(pool, workspace_id, p1, child_id, admin_id).await;

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
        for pid in [self.p1, self.p2, self.p3] {
            sqlx::query("DELETE FROM project_pages WHERE project_id = $1")
                .bind(pid)
                .execute(pool)
                .await
                .ok();
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
        sqlx::query("DELETE FROM pages WHERE workspace_id = $1")
            .bind(self.workspace_id)
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
    let mut out: Vec<String> = body["results"]
        .as_array()
        .expect("envelope has results")
        .iter()
        .map(|row| row["name"].as_str().unwrap_or("").to_string())
        .collect();
    // Default ordering is -updated_at, which ties within a scratch fixture;
    // sort so assertions are deterministic.
    out.sort();
    out
}

fn project_ids_of(body: &serde_json::Value, name: &str) -> Vec<String> {
    body["results"]
        .as_array()
        .expect("envelope has results")
        .iter()
        .find(|row| row["name"].as_str() == Some(name))
        .map(|row| {
            row["project_ids"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|v| v.as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

async fn call(
    st: &AppState,
    user: Uuid,
    slug: &str,
    q: WsPagesQuery,
) -> (StatusCode, serde_json::Value) {
    let (status, Json(body)) = workspace_pages(State(st.clone()), AuthUser(user), Path(slug.to_string()), Query(q))
        .await
        .unwrap();
    (status, body)
}

fn all() -> WsPagesQuery {
    WsPagesQuery {
        type_: None,
        project_id: None,
        search: None,
        per_page: None,
        cursor: None,
        order_by: None,
    }
}

#[tokio::test]
async fn non_workspace_member_gets_403() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, _) = call(&st, scratch.outsider_id, &scratch.slug, all()).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn non_project_member_gets_empty_list() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(&st, scratch.mallory_id, &scratch.slug, all()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["results"].as_array().unwrap().len(), 0);
    assert_eq!(body["total_results"], 0);
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn all_tab_excludes_private_of_others_and_archived() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(&st, scratch.admin_id, &scratch.slug, all()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        names(&body),
        vec![
            "Alice Private".to_string(),
            "Alpha Guide".to_string(),
            "Beta Manual".to_string(),
            "Child Note".to_string(),
            "Multi SOP".to_string(),
        ]
    );
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn public_tab_lists_public_unarchived_only() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(
        &st,
        scratch.admin_id,
        &scratch.slug,
        WsPagesQuery {
            type_: Some("public".to_string()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        names(&body),
        vec![
            "Alpha Guide".to_string(),
            "Beta Manual".to_string(),
            "Child Note".to_string(),
            "Multi SOP".to_string(),
        ]
    );
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn private_tab_lists_only_own_private_pages() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(
        &st,
        scratch.admin_id,
        &scratch.slug,
        WsPagesQuery {
            type_: Some("private".to_string()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(names(&body), vec!["Alice Private".to_string()]);
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn archived_tab_lists_only_archived_pages() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(
        &st,
        scratch.admin_id,
        &scratch.slug,
        WsPagesQuery {
            type_: Some("archived".to_string()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(names(&body), vec!["Old Runbook".to_string()]);
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn invalid_type_falls_back_to_all() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(
        &st,
        scratch.admin_id,
        &scratch.slug,
        WsPagesQuery {
            type_: Some("nope".to_string()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(names(&body).len(), 5);
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn project_filter_scopes_links_and_still_enforces_visibility() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(
        &st,
        scratch.admin_id,
        &scratch.slug,
        WsPagesQuery {
            project_id: Some(scratch.p2),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // Beta Manual + Multi SOP (linked to p2); Carol Private stays hidden
    // even though it is linked to p2.
    assert_eq!(
        names(&body),
        vec!["Beta Manual".to_string(), "Multi SOP".to_string()]
    );
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn restricted_guest_sees_only_own_pages() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(&st, scratch.carol_id, &scratch.slug, all()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(names(&body), vec!["Carol Private".to_string()]);
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn guest_in_open_project_sees_public_pages() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(&st, scratch.dave_id, &scratch.slug, all()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        names(&body),
        vec![
            "Alpha Guide".to_string(),
            "Child Note".to_string(),
            "Multi SOP".to_string(),
        ]
    );
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn search_filters_by_name() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(
        &st,
        scratch.admin_id,
        &scratch.slug,
        WsPagesQuery {
            search: Some("guide".to_string()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(names(&body), vec!["Alpha Guide".to_string()]);
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn pagination_cursors_page_results() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, first) = call(
        &st,
        scratch.admin_id,
        &scratch.slug,
        WsPagesQuery {
            per_page: Some("2".to_string()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["total_results"], 5);
    assert_eq!(first["next_page_results"], true);
    let first_names = names(&first);
    assert_eq!(first_names.len(), 2);

    let cursor = first["next_cursor"].as_str().unwrap().to_string();
    let (status, second) = call(
        &st,
        scratch.admin_id,
        &scratch.slug,
        WsPagesQuery {
            per_page: Some("2".to_string()),
            cursor: Some(cursor),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let second_names = names(&second);
    assert_eq!(second_names.len(), 2);
    for name in &second_names {
        assert!(!first_names.contains(name));
    }
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn multi_linked_page_appears_once_with_member_projects() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(&st, scratch.admin_id, &scratch.slug, all()).await;
    assert_eq!(status, StatusCode::OK);
    let occurrences = names(&body).iter().filter(|n| *n == "Multi SOP").count();
    assert_eq!(occurrences, 1);
    let mut ids = project_ids_of(&body, "Multi SOP");
    ids.sort();
    let mut expected = vec![scratch.p1.to_string(), scratch.p2.to_string()];
    expected.sort();
    assert_eq!(ids, expected);
    scratch.cleanup(&pool).await;
}

#[tokio::test]
async fn child_pages_carry_parent_field() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    let (status, body) = call(&st, scratch.admin_id, &scratch.slug, all()).await;
    assert_eq!(status, StatusCode::OK);
    let child = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"].as_str() == Some("Child Note"))
        .expect("child page is listed");
    assert!(child["parent"].is_string());
    scratch.cleanup(&pool).await;
}
