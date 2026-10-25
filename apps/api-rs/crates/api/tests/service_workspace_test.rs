//! Workspace-level services backend integration tests (design 2026-10-17).
//! DB-backed; run serially:
//! `DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
//!  cargo test -p api --test service_workspace_test -- --test-threads=1`

use api::middleware::auth::AuthUser;
use api::state::AppState;
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

#[allow(dead_code)] // used from Task BE-3 onward
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

async fn insert_project(
    pool: &PgPool,
    project_id: Uuid,
    workspace_id: Uuid,
    name: &str,
    identifier: &str,
) {
    sqlx::query(
        "INSERT INTO projects (id, created_at, updated_at, name, description, network, \
         identifier, workspace_id, cycle_view, module_view, issue_views_view, page_view, \
         intake_view, archive_in, close_in, logo_props, is_time_tracking_enabled, \
         is_issue_type_enabled, guest_view_all_features, timezone) \
         VALUES ($1, now(), now(), $2, '', 2, $3, $4, false, false, false, \
         false, false, 30, 30, '{}'::jsonb, false, false, false, 'UTC')",
    )
    .bind(project_id)
    .bind(name)
    .bind(identifier)
    .bind(workspace_id)
    .execute(pool)
    .await
    .expect("scratch project");
}

async fn insert_service(pool: &PgPool, workspace_id: Uuid, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO services (id, workspace_id, name, description, description_html, status, \
         criticality, \"type\", sort_order, created_at, updated_at) \
         VALUES ($1, $2, $3, '', '', 'active', 'medium', 'internal', 65535, now(), now())",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(name)
    .execute(pool)
    .await
    .expect("scratch service");
    id
}

async fn set_service_position(pool: &PgPool, service_id: Uuid, x: f64, y: f64) {
    sqlx::query(
        "UPDATE services SET position = jsonb_build_object('x', $2::float8, 'y', $3::float8) \
         WHERE id = $1",
    )
    .bind(service_id)
    .bind(x)
    .bind(y)
    .execute(pool)
    .await
    .expect("set service position");
}

async fn service_position(pool: &PgPool, service_id: Uuid) -> Option<serde_json::Value> {
    sqlx::query_scalar("SELECT position FROM services WHERE id = $1")
        .bind(service_id)
        .fetch_one(pool)
        .await
        .expect("service position")
}

async fn link_service(
    pool: &PgPool,
    service_id: Uuid,
    project_id: Uuid,
    workspace_id: Uuid,
    link_type: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO service_project_links (id, workspace_id, service_id, project_id, link_type, \
         created_at, updated_at) VALUES ($1, $2, $3, $4, $5, now(), now())",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(service_id)
    .bind(project_id)
    .bind(link_type)
    .execute(pool)
    .await
    .expect("scratch service link");
    id
}

struct Scratch {
    slug: String,
    workspace_id: Uuid,
    p1: Uuid,
    p2: Uuid,
    p4: Uuid,
    admin: Uuid,
    owner_admin: Uuid,
    owner_member: Uuid,
    consumer_admin: Uuid,
    guest: Uuid,
    outsider: Uuid,
    s1: Uuid, // owned by P1, consumer link P2
    s2: Uuid, // owned by P2
    s4: Uuid, // owned by P4
}

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let slug = format!("svc-{}", Uuid::new_v4().simple());
        let workspace_id = Uuid::new_v4();
        let p1 = Uuid::new_v4();
        let p2 = Uuid::new_v4();
        let p4 = Uuid::new_v4();
        let admin = Uuid::new_v4();
        let owner_admin = Uuid::new_v4();
        let owner_member = Uuid::new_v4();
        let consumer_admin = Uuid::new_v4();
        let guest = Uuid::new_v4();
        let outsider = Uuid::new_v4();

        for (id, name) in [
            (admin, "admin"),
            (owner_admin, "owner-admin"),
            (owner_member, "owner-member"),
            (consumer_admin, "consumer-admin"),
            (guest, "guest"),
            (outsider, "outsider"),
        ] {
            insert_user(pool, id, &format!("{slug}-{name}")).await;
        }

        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'Svc Scratch', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
        )
        .bind(workspace_id)
        .bind(&slug)
        .bind(admin)
        .execute(pool)
        .await
        .expect("scratch workspace");

        // Workspace roles: admin = ws admin (no project rows); owner/consumer/outsider
        // are ws members (15); guest is ws guest (5).
        insert_workspace_member(pool, admin, workspace_id, 20).await;
        for user in [owner_admin, owner_member, consumer_admin, outsider] {
            insert_workspace_member(pool, user, workspace_id, 15).await;
        }
        insert_workspace_member(pool, guest, workspace_id, 5).await;

        let project_specs: [(Uuid, &str, &str); 3] = [
            (p1, "Scratch One", "P1"),
            (p2, "Scratch Two", "P2"),
            (p4, "Scratch Four", "P4"),
        ];
        for (project_id, name, ident) in project_specs {
            insert_project(pool, project_id, workspace_id, name, ident).await;
        }
        insert_project_member(pool, owner_admin, p1, workspace_id, 20).await;
        insert_project_member(pool, owner_member, p1, workspace_id, 15).await;
        insert_project_member(pool, guest, p1, workspace_id, 5).await;
        insert_project_member(pool, consumer_admin, p2, workspace_id, 20).await;
        insert_project_member(pool, outsider, p4, workspace_id, 15).await;

        let s1 = insert_service(pool, workspace_id, "ISO Switch").await;
        link_service(pool, s1, p1, workspace_id, "owner").await;
        link_service(pool, s1, p2, workspace_id, "consumer").await;
        let s2 = insert_service(pool, workspace_id, "Core Banking").await;
        link_service(pool, s2, p2, workspace_id, "owner").await;
        let s4 = insert_service(pool, workspace_id, "P4 Widget").await;
        link_service(pool, s4, p4, workspace_id, "owner").await;

        Self {
            slug,
            workspace_id,
            p1,
            p2,
            p4,
            admin,
            owner_admin,
            owner_member,
            consumer_admin,
            guest,
            outsider,
            s1,
            s2,
            s4,
        }
    }
}

#[tokio::test]
async fn visibility_matrix_and_gates() {
    use api::routes::service_access::{
        full_visible, gate_admin, gate_writer, linked_to_project, visible_service_ids,
    };

    let pool = pool().await;
    let sc = Scratch::new(&pool).await;

    let mut admin = visible_service_ids(&pool, sc.admin, &sc.slug)
        .await
        .unwrap();
    admin.sort();
    let mut all = vec![sc.s1, sc.s2, sc.s4];
    all.sort();
    assert_eq!(admin, all, "ws admin sees every service");

    assert_eq!(
        visible_service_ids(&pool, sc.owner_member, &sc.slug)
            .await
            .unwrap(),
        vec![sc.s1],
        "P1 member sees only S1"
    );
    let mut consumer = visible_service_ids(&pool, sc.consumer_admin, &sc.slug)
        .await
        .unwrap();
    consumer.sort();
    let mut consumer_expected = vec![sc.s1, sc.s2];
    consumer_expected.sort();
    assert_eq!(
        consumer, consumer_expected,
        "P2 admin sees S1 (consumer) + S2 (owner)"
    );
    assert_eq!(
        visible_service_ids(&pool, sc.guest, &sc.slug)
            .await
            .unwrap(),
        vec![sc.s1],
        "guest sees linked services read-only"
    );
    assert_eq!(
        visible_service_ids(&pool, sc.outsider, &sc.slug)
            .await
            .unwrap(),
        vec![sc.s4],
        "outsider sees only P4"
    );

    assert!(full_visible(&pool, sc.admin, &sc.slug, sc.s4)
        .await
        .unwrap());
    assert!(full_visible(&pool, sc.owner_member, &sc.slug, sc.s1)
        .await
        .unwrap());
    assert!(full_visible(&pool, sc.consumer_admin, &sc.slug, sc.s1)
        .await
        .unwrap());
    assert!(!full_visible(&pool, sc.outsider, &sc.slug, sc.s1)
        .await
        .unwrap());
    assert!(!full_visible(&pool, sc.guest, &sc.slug, sc.s2)
        .await
        .unwrap());

    assert!(gate_writer(&pool, sc.owner_member, &sc.slug, sc.s1)
        .await
        .unwrap());
    assert!(gate_writer(&pool, sc.admin, &sc.slug, sc.s1).await.unwrap());
    assert!(!gate_writer(&pool, sc.consumer_admin, &sc.slug, sc.s1)
        .await
        .unwrap());
    assert!(!gate_writer(&pool, sc.guest, &sc.slug, sc.s1).await.unwrap());
    assert!(!gate_writer(&pool, sc.outsider, &sc.slug, sc.s1)
        .await
        .unwrap());

    assert!(gate_admin(&pool, sc.owner_admin, &sc.slug, sc.s1)
        .await
        .unwrap());
    assert!(gate_admin(&pool, sc.admin, &sc.slug, sc.s1).await.unwrap());
    assert!(!gate_admin(&pool, sc.owner_member, &sc.slug, sc.s1)
        .await
        .unwrap());
    assert!(!gate_admin(&pool, sc.consumer_admin, &sc.slug, sc.s1)
        .await
        .unwrap());

    assert!(linked_to_project(&pool, sc.s1, sc.p2).await.unwrap());
    assert!(!linked_to_project(&pool, sc.s1, sc.p4).await.unwrap());
}

#[tokio::test]
async fn canonical_list_scopes_and_filters() {
    use api::routes::service::{list_workspace, WorkspaceServiceListParams};
    use axum::extract::{Path, Query, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    let (status, axum::Json(body)) = list_workspace(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path(sc.slug.clone()),
        Query(WorkspaceServiceListParams::default()),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::OK);
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 1, "outsider sees only P4-owned service");
    assert_eq!(rows[0]["name"], "P4 Widget");
    assert_eq!(rows[0]["owner_project_id"], sc.p4.to_string());
    assert_eq!(rows[0]["project_id"], sc.p4.to_string(), "deprecated alias");
    assert_eq!(rows[0]["access_level"], "full");

    let (_, axum::Json(body)) = list_workspace(
        State(st.clone()),
        AuthUser(sc.admin),
        Path(sc.slug.clone()),
        Query(WorkspaceServiceListParams::default()),
    )
    .await
    .unwrap();
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 3, "ws admin sees all");
    let s1 = rows.iter().find(|r| r["id"] == sc.s1.to_string()).unwrap();
    assert_eq!(s1["project_links"].as_array().unwrap().len(), 2);

    let (_, axum::Json(body)) = list_workspace(
        State(st.clone()),
        AuthUser(sc.admin),
        Path(sc.slug.clone()),
        Query(WorkspaceServiceListParams {
            project: Some(sc.p2),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 2, "P2 links: S1 (consumer) + S2 (owner)");

    let (_, axum::Json(body)) = list_workspace(
        State(st.clone()),
        AuthUser(sc.admin),
        Path(sc.slug.clone()),
        Query(WorkspaceServiceListParams {
            project: Some(sc.p2),
            link: Some("owner".into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], sc.s2.to_string());
}

#[tokio::test]
async fn canonical_detail_is_full_only() {
    use api::routes::service::detail_workspace;
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    let (status, _) = detail_workspace(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path((sc.slug.clone(), sc.s1)),
    )
    .await
    .unwrap();
    assert_eq!(
        status,
        axum::http::StatusCode::NOT_FOUND,
        "outsider cannot read S1 (404, not 403)"
    );

    let (status, axum::Json(body)) = detail_workspace(
        State(st.clone()),
        AuthUser(sc.consumer_admin),
        Path((sc.slug.clone(), sc.s1)),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body["access_level"], "full");
    assert_eq!(body["owner_project_id"], sc.p1.to_string());
}

#[tokio::test]
async fn canonical_create_patch_destroy() {
    use api::routes::service::{
        create_workspace, destroy_workspace, detail_workspace, patch_workspace, CreateService,
        CreateWorkspaceService, PatchService,
    };
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    let new_service = |name: &str| CreateWorkspaceService {
        owner_project_id: sc.p1,
        service: CreateService {
            name: Some(name.into()),
            description: None,
            description_html: None,
            status: None,
            criticality: None,
            service_type: None,
            owner_id: None,
            repository_url: None,
            documentation_url: None,
        },
    };

    // outsider cannot create in P1
    let denied = create_workspace(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path(sc.slug.clone()),
        Json(new_service("Nope")),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, axum::http::StatusCode::FORBIDDEN);

    // P1 member creates; owner link is created atomically
    let (status, axum::Json(created)) = create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(new_service("Payments API")),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    assert_eq!(created["owner_project_id"], sc.p1.to_string());
    let new_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    assert!(
        api::routes::service_access::linked_to_project(&st.pool, new_id, sc.p1)
            .await
            .unwrap(),
        "owner link exists"
    );

    // duplicate name inside the workspace (renamed case) is rejected
    let (status, axum::Json(dup)) = create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(new_service("payments api")),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(dup["error"], "A service with this name already exists.");

    // writer patch; consumer admin cannot
    let consumer_patch = patch_workspace(
        State(st.clone()),
        AuthUser(sc.consumer_admin),
        Path((sc.slug.clone(), new_id)),
        Json(PatchService {
            status: Some("maintenance".into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(consumer_patch.0, axum::http::StatusCode::FORBIDDEN);

    let (status, axum::Json(patched)) = patch_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), new_id)),
        Json(PatchService {
            status: Some("maintenance".into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(patched["status"], "maintenance");

    // delete: member+ denied, project admin allowed
    let member_delete = destroy_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), new_id)),
    )
    .await
    .unwrap();
    assert_eq!(member_delete.0, axum::http::StatusCode::FORBIDDEN);

    let admin_delete = destroy_workspace(
        State(st.clone()),
        AuthUser(sc.owner_admin),
        Path((sc.slug.clone(), new_id)),
    )
    .await
    .unwrap();
    assert_eq!(admin_delete.0, axum::http::StatusCode::NO_CONTENT);

    let gone = detail_workspace(
        State(st.clone()),
        AuthUser(sc.admin),
        Path((sc.slug.clone(), new_id)),
    )
    .await
    .unwrap();
    assert_eq!(
        gone.0,
        axum::http::StatusCode::NOT_FOUND,
        "soft-deleted service is invisible"
    );
}

#[tokio::test]
async fn service_mutations_reset_workspace_layout() {
    use api::routes::service::{
        create_workspace, destroy_workspace, CreateService, CreateWorkspaceService,
    };
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    let new_service = |name: &str| CreateWorkspaceService {
        owner_project_id: sc.p1,
        service: CreateService {
            name: Some(name.into()),
            description: None,
            description_html: None,
            status: None,
            criticality: None,
            service_type: None,
            owner_id: None,
            repository_url: None,
            documentation_url: None,
        },
    };

    for id in [sc.s1, sc.s2, sc.s4] {
        set_service_position(&st.pool, id, 10.0, 20.0).await;
    }
    let updated_at_before: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT updated_at FROM services WHERE id = $1")
            .bind(sc.s1)
            .fetch_one(&st.pool)
            .await
            .expect("service updated_at");

    let (status, Json(created)) = create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(new_service("Payments API")),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let new_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    for id in [sc.s1, sc.s2, sc.s4, new_id] {
        assert_eq!(
            service_position(&st.pool, id).await,
            None,
            "create service resets every workspace position"
        );
    }
    let updated_at_after: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT updated_at FROM services WHERE id = $1")
            .bind(sc.s1)
            .fetch_one(&st.pool)
            .await
            .expect("service updated_at");
    assert_eq!(
        updated_at_before, updated_at_after,
        "layout reset must not touch updated_at"
    );

    set_service_position(&st.pool, sc.s1, 30.0, 40.0).await;
    set_service_position(&st.pool, new_id, 50.0, 60.0).await;
    let (status, _) = destroy_workspace(
        State(st.clone()),
        AuthUser(sc.owner_admin),
        Path((sc.slug.clone(), new_id)),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
    for id in [sc.s1, sc.s2, sc.s4] {
        assert_eq!(
            service_position(&st.pool, id).await,
            None,
            "delete service resets remaining workspace positions"
        );
    }
}

#[tokio::test]
async fn project_links_and_transfer_owner() {
    use api::routes::service::{
        project_links_create, project_links_destroy, project_links_list, transfer_owner,
        CreateProjectLink, TransferOwner,
    };
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    // P4 member (15) cannot self-serve a consumer link
    let denied = project_links_create(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path((sc.slug.clone(), sc.s1)),
        Json(CreateProjectLink {
            project_id: sc.p4,
            link_type: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, axum::http::StatusCode::FORBIDDEN);

    // promote to P4 admin, retry -> self-serve works
    sqlx::query("UPDATE project_members SET role = 20 WHERE member_id = $1 AND project_id = $2")
        .bind(sc.outsider)
        .bind(sc.p4)
        .execute(&st.pool)
        .await
        .unwrap();
    let (status, Json(link)) = project_links_create(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path((sc.slug.clone(), sc.s1)),
        Json(CreateProjectLink {
            project_id: sc.p4,
            link_type: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    assert_eq!(link["link_type"], "consumer");
    let link_id = Uuid::parse_str(link["id"].as_str().unwrap()).unwrap();

    // duplicate active link rejected
    let dup = project_links_create(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path((sc.slug.clone(), sc.s1)),
        Json(CreateProjectLink {
            project_id: sc.p4,
            link_type: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(dup.0, axum::http::StatusCode::BAD_REQUEST);

    // owner admin can revoke it
    let (status, _) = project_links_destroy(
        State(st.clone()),
        AuthUser(sc.owner_admin),
        Path((sc.slug.clone(), sc.s1, link_id)),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
    assert!(
        !api::routes::service_access::linked_to_project(&st.pool, sc.s1, sc.p4)
            .await
            .unwrap()
    );

    // owner link cannot be deleted directly
    let (_, Json(links)) = project_links_list(
        State(st.clone()),
        AuthUser(sc.admin),
        Path((sc.slug.clone(), sc.s1)),
    )
    .await
    .unwrap();
    let owner_link_id = links
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["link_type"] == "owner")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let owner_del = project_links_destroy(
        State(st.clone()),
        AuthUser(sc.admin),
        Path((
            sc.slug.clone(),
            sc.s1,
            Uuid::parse_str(&owner_link_id).unwrap(),
        )),
    )
    .await
    .unwrap();
    assert_eq!(owner_del.0, axum::http::StatusCode::BAD_REQUEST);

    // transfer: non ws-admin denied
    let denied = transfer_owner(
        State(st.clone()),
        AuthUser(sc.owner_admin),
        Path((sc.slug.clone(), sc.s1)),
        Json(TransferOwner { project_id: sc.p4 }),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, axum::http::StatusCode::FORBIDDEN);

    // re-add P4 consumer link so transfer can absorb it
    project_links_create(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path((sc.slug.clone(), sc.s1)),
        Json(CreateProjectLink {
            project_id: sc.p4,
            link_type: None,
        }),
    )
    .await
    .unwrap();

    // ws admin transfers; old owner demoted to consumer; target link absorbed
    let (status, Json(after)) = transfer_owner(
        State(st.clone()),
        AuthUser(sc.admin),
        Path((sc.slug.clone(), sc.s1)),
        Json(TransferOwner { project_id: sc.p4 }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(after["owner_project_id"], sc.p4.to_string());
    let links = after["project_links"].as_array().unwrap();
    assert_eq!(
        links
            .iter()
            .filter(|l| l["project_id"] == sc.p4.to_string())
            .count(),
        1,
        "absorbed consumer link: exactly one P4 link (owner)"
    );
    assert!(links
        .iter()
        .any(|l| l["project_id"] == sc.p4.to_string() && l["link_type"] == "owner"));
    assert!(links
        .iter()
        .any(|l| l["project_id"] == sc.p1.to_string() && l["link_type"] == "consumer"));
    assert!(links
        .iter()
        .any(|l| l["project_id"] == sc.p2.to_string() && l["link_type"] == "consumer"));
}

#[tokio::test]
async fn canonical_dependencies_cross_project_with_lite() {
    use api::routes::service::{
        dependencies_create_workspace, dependencies_list_workspace, CreateDependency,
    };
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    // S1 -> S4: S4 is not full-visible to a P1 member -> rejected
    let denied = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(CreateDependency {
            from_service_id: sc.s1,
            to_service_id: sc.s4,
            dependency_type: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, axum::http::StatusCode::BAD_REQUEST);

    // S2 -> S1 by P2 admin: S1 is consumer-linked to P2, so both endpoints are full
    let (status, Json(edge)) = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.consumer_admin),
        Path(sc.slug.clone()),
        Json(CreateDependency {
            from_service_id: sc.s2,
            to_service_id: sc.s1,
            dependency_type: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    assert_eq!(edge["from_service"]["access_level"], "full");
    assert_eq!(edge["to_service"]["access_level"], "full");

    // outsider sees no edges (neither endpoint visible to P4)
    let (_, Json(rows)) = dependencies_list_workspace(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path(sc.slug.clone()),
    )
    .await
    .unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 0);

    // P1 member sees the S2 -> S1 edge with S2 projected lite
    let (_, Json(rows)) = dependencies_list_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
    )
    .await
    .unwrap();
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["from_service_id"], sc.s2.to_string());
    assert_eq!(rows[0]["to_service_id"], sc.s1.to_string());
    assert_eq!(rows[0]["from_service"]["access_level"], "lite");
    assert_eq!(rows[0]["from_service"]["name"], "Core Banking");
    assert!(
        rows[0]["from_service"].get("owner_id").is_none(),
        "lite hides details"
    );
    assert_eq!(rows[0]["to_service"]["access_level"], "full");

    // cycle: P1 subscribes to S2, then S1 -> S2 closes S2 -> S1
    link_service(&st.pool, sc.s2, sc.p1, sc.workspace_id, "consumer").await;
    let cycle = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(CreateDependency {
            from_service_id: sc.s1,
            to_service_id: sc.s2,
            dependency_type: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(cycle.0, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(cycle.1["error"], "This dependency would create a cycle.");
}

#[tokio::test]
async fn canonical_dependency_destroy_by_from_writer() {
    use api::routes::service::{
        dependencies_create_workspace, dependency_destroy_workspace, CreateDependency,
    };
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    // P1 subscribes to S2 so its member may create S1 -> S2
    link_service(&st.pool, sc.s2, sc.p1, sc.workspace_id, "consumer").await;
    let (status, Json(edge)) = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(CreateDependency {
            from_service_id: sc.s1,
            to_service_id: sc.s2,
            dependency_type: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let edge_id = Uuid::parse_str(edge["id"].as_str().unwrap()).unwrap();

    // outsider cannot delete it
    let denied = dependency_destroy_workspace(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path((sc.slug.clone(), edge_id)),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, axum::http::StatusCode::FORBIDDEN);

    // from-side writer can
    let (status, _) = dependency_destroy_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), edge_id)),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn canonical_dependency_type_default_and_validation() {
    use api::routes::service::{dependencies_create_workspace, CreateDependency};
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    // P1 must see S2 as full before it can depend on it.
    link_service(&st.pool, sc.s2, sc.p1, sc.workspace_id, "consumer").await;

    let make = |dependency_type: Option<&str>| CreateDependency {
        from_service_id: sc.s1,
        to_service_id: sc.s2,
        dependency_type: dependency_type.map(str::to_string),
    };

    // Omitted type defaults to depends_on.
    let (status, Json(edge)) = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(make(None)),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    assert_eq!(edge["dependency_type"], "depends_on");

    // Same pair with a different type is allowed.
    let (status, Json(uses)) = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(make(Some("uses"))),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    assert_eq!(uses["dependency_type"], "uses");

    // Same triple is a duplicate.
    let duplicate = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(make(Some("uses"))),
    )
    .await
    .unwrap();
    assert_eq!(duplicate.0, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(duplicate.1["error"], "This dependency already exists.");

    // Unknown type is rejected.
    let invalid = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(make(Some("eats"))),
    )
    .await
    .unwrap();
    assert_eq!(invalid.0, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(invalid.1["error"], "Invalid dependency type.");
}

#[tokio::test]
async fn canonical_dependency_patch_type() {
    use api::routes::service::{
        dependencies_create_workspace, dependency_update_workspace, CreateDependency,
        UpdateDependency,
    };
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;
    link_service(&st.pool, sc.s2, sc.p1, sc.workspace_id, "consumer").await;

    let (_, Json(edge)) = dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(CreateDependency {
            from_service_id: sc.s1,
            to_service_id: sc.s2,
            dependency_type: None,
        }),
    )
    .await
    .unwrap();
    let edge_id = Uuid::parse_str(edge["id"].as_str().unwrap()).unwrap();

    let (status, Json(patched)) = dependency_update_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), edge_id)),
        Json(UpdateDependency {
            dependency_type: Some("runs_on".into()),
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(patched["dependency_type"], "runs_on");

    // Create a second edge, then collide the PATCH with it.
    dependencies_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(CreateDependency {
            from_service_id: sc.s1,
            to_service_id: sc.s2,
            dependency_type: Some("uses".into()),
        }),
    )
    .await
    .unwrap();

    let conflict = dependency_update_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), edge_id)),
        Json(UpdateDependency {
            dependency_type: Some("uses".into()),
        }),
    )
    .await
    .unwrap();
    assert_eq!(conflict.0, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(conflict.1["error"], "This dependency already exists.");

    let invalid = dependency_update_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), edge_id)),
        Json(UpdateDependency {
            dependency_type: Some("eats".into()),
        }),
    )
    .await
    .unwrap();
    assert_eq!(invalid.0, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(invalid.1["error"], "Invalid dependency type.");

    // Outsider cannot patch.
    let denied = dependency_update_workspace(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path((sc.slug.clone(), edge_id)),
        Json(UpdateDependency {
            dependency_type: Some("depends_on".into()),
        }),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, axum::http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn compat_services_scope_to_linked_and_owner_gates() {
    use api::routes::service::{
        detail as compat_detail, list as compat_list, patch as compat_patch,
    };
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    // consumer project list includes the consumer-linked service
    let (status, Json(rows)) = compat_list(
        State(st.clone()),
        AuthUser(sc.consumer_admin),
        Path((sc.slug.clone(), sc.p2)),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::OK);
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let s1 = rows.iter().find(|r| r["id"] == sc.s1.to_string()).unwrap();
    assert_eq!(
        s1["project_id"],
        sc.p1.to_string(),
        "deprecated field = owner project"
    );

    // consumer cannot patch through the compat route (owner-only writer)
    let denied = compat_patch(
        State(st.clone()),
        AuthUser(sc.consumer_admin),
        Path((sc.slug.clone(), sc.p2, sc.s1)),
        Json(api::routes::service::PatchService {
            status: Some("maintenance".into()),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, axum::http::StatusCode::FORBIDDEN);

    // outsider is a P4 member: gate passes, linked assertion 404s
    let not_linked = compat_detail(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path((sc.slug.clone(), sc.p4, sc.s1)),
    )
    .await
    .unwrap();
    assert_eq!(not_linked.0, axum::http::StatusCode::NOT_FOUND);

    // non-member is denied at the gate
    let stranger = compat_detail(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p4, sc.s4)),
    )
    .await
    .unwrap();
    assert_eq!(stranger.0, axum::http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn compat_dependencies_echo_and_link_scope() {
    use api::routes::service::{
        dependencies_create as compat_deps_create, dependencies_list as compat_deps_list,
        CreateDependency,
    };
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    // S2 is not linked to P1 -> target rejected
    let denied = compat_deps_create(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1)),
        Json(CreateDependency {
            from_service_id: sc.s1,
            to_service_id: sc.s2,
            dependency_type: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(denied.1["error"], "Target service not found.");

    // link S2 to P1 as consumer, then the same create succeeds
    link_service(&st.pool, sc.s2, sc.p1, sc.workspace_id, "consumer").await;
    let (status, Json(edge)) = compat_deps_create(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1)),
        Json(CreateDependency {
            from_service_id: sc.s1,
            to_service_id: sc.s2,
            dependency_type: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    assert_eq!(edge["project_id"], sc.p1.to_string(), "compat echo");

    let (_, Json(rows)) = compat_deps_list(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1)),
    )
    .await
    .unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["project_id"], sc.p1.to_string());
}

async fn insert_issue(pool: &PgPool, sc: &Scratch, project_id: Uuid, name: &str) -> Uuid {
    let state_id = Uuid::new_v4();
    let type_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO states (id, name, description, color, slug, project_id, workspace_id, \
         sequence, \"group\", \"default\", is_triage, created_at, updated_at) \
         VALUES ($1, 'Open', '', '#cccccc', $2, $3, $4, 65535, 'unstarted', true, false, now(), now())",
    )
    .bind(state_id)
    .bind(format!("open-{}", Uuid::new_v4().simple()))
    .bind(project_id)
    .bind(sc.workspace_id)
    .execute(pool)
    .await
    .expect("scratch state");
    sqlx::query(
        "INSERT INTO issue_types (id, name, description, logo_props, workspace_id, is_active, \
         is_default, level, is_epic, requires_service, created_at, updated_at) \
         VALUES ($1, 'Incident', '', '{}'::jsonb, $2, true, true, 0, false, true, now(), now())",
    )
    .bind(type_id)
    .bind(sc.workspace_id)
    .execute(pool)
    .await
    .expect("scratch type");
    let issue_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, name, description_html, description_json, description_stripped, \
         priority, is_draft, sort_order, sequence_id, state_id, type_id, project_id, workspace_id, \
         created_at, updated_at) \
         VALUES ($1, $2, '<p></p>', '{}', '', 'none', false, 65535, \
         (SELECT COALESCE(MAX(sequence_id), 0) + 1 FROM issues WHERE project_id = $4), \
         $3, $7, $4, $5, now(), now())",
    )
    .bind(issue_id)
    .bind(name)
    .bind(state_id)
    .bind(project_id)
    .bind(sc.workspace_id)
    .bind(1_i64)
    .bind(type_id)
    .execute(pool)
    .await
    .expect("scratch issue");
    issue_id
}

#[tokio::test]
async fn service_issues_require_linked_service() {
    use api::routes::service::{issues_create as compat_issues_create, CreateServiceIssue};
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;
    let issue_id = insert_issue(&st.pool, &sc, sc.p1, "ISO one").await;

    // S2 is not linked to P1 -> 404
    let denied = compat_issues_create(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1)),
        Json(CreateServiceIssue {
            service_id: sc.s2,
            issue_id,
        }),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, axum::http::StatusCode::NOT_FOUND);

    // S1 is linked -> ok
    let (status, Json(link)) = compat_issues_create(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1)),
        Json(CreateServiceIssue {
            service_id: sc.s1,
            issue_id,
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    assert_eq!(link["service_id"], sc.s1.to_string());
}

#[tokio::test]
async fn canonical_service_issues_resolve_project_from_issue() {
    use api::routes::service::{
        issue_destroy_workspace, issues_create_workspace, issues_list_workspace,
        CreateServiceIssue, ServiceIssueListParams,
    };
    use axum::extract::{Path, Query, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;
    let issue_id = insert_issue(&st.pool, &sc, sc.p1, "ISO two").await;

    // outsider is not a member of P1
    let denied = issues_create_workspace(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path(sc.slug.clone()),
        Json(CreateServiceIssue {
            service_id: sc.s1,
            issue_id,
        }),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, axum::http::StatusCode::FORBIDDEN);

    let (status, Json(link)) = issues_create_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Json(CreateServiceIssue {
            service_id: sc.s1,
            issue_id,
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let link_id = Uuid::parse_str(link["id"].as_str().unwrap()).unwrap();

    // canonical list filtered by service_id
    let (_, Json(rows)) = issues_list_workspace(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
        Query(ServiceIssueListParams {
            service_id: Some(sc.s1),
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);

    // destroy: consumer admin is not a member of the issue's project (P1) -> denied
    let denied = issue_destroy_workspace(
        State(st.clone()),
        AuthUser(sc.consumer_admin),
        Path((sc.slug.clone(), link_id)),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, axum::http::StatusCode::FORBIDDEN);
}

async fn insert_war_room(pool: &PgPool, sc: &Scratch, project_id: Uuid) -> Uuid {
    let issue_id = insert_issue(pool, sc, project_id, "War room primary").await;
    let room_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO war_rooms (id, workspace_id, project_id, sequence_id, name, severity, \
         status, primary_issue_id, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, 'Incident room', 'sev3', 'active', $5, now(), now())",
    )
    .bind(room_id)
    .bind(sc.workspace_id)
    .bind(project_id)
    .bind(1_i64)
    .bind(issue_id)
    .execute(pool)
    .await
    .expect("scratch war room");
    room_id
}

async fn insert_incident(
    pool: &PgPool,
    sc: &Scratch,
    project_id: Uuid,
    name: &str,
    priority: &str,
    group: &str,
) -> Uuid {
    let state_id = Uuid::new_v4();
    let type_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO states (id, name, description, color, slug, project_id, workspace_id, \
         sequence, \"group\", \"default\", is_triage, created_at, updated_at) \
         VALUES ($1, $2, '', '#cccccc', $3, $4, $5, 65535, $6, false, false, now(), now())",
    )
    .bind(state_id)
    .bind(format!("health-{}", Uuid::new_v4().simple()))
    .bind(format!("health-{}", Uuid::new_v4().simple()))
    .bind(project_id)
    .bind(sc.workspace_id)
    .bind(group)
    .execute(pool)
    .await
    .expect("scratch incident state");
    sqlx::query(
        "INSERT INTO issue_types (id, name, description, logo_props, workspace_id, is_active, \
         is_default, level, is_epic, requires_service, created_at, updated_at) \
         VALUES ($1, 'Incident', '', '{}'::jsonb, $2, true, true, 0, false, true, now(), now())",
    )
    .bind(type_id)
    .bind(sc.workspace_id)
    .execute(pool)
    .await
    .expect("scratch incident type");
    let issue_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, name, description_html, description_json, description_stripped, \
         priority, is_draft, sort_order, sequence_id, state_id, type_id, project_id, workspace_id, \
         created_at, updated_at) \
         VALUES ($1, $2, '<p></p>', '{}', '', $3, false, 65535, \
         (SELECT COALESCE(MAX(sequence_id), 0) + 1 FROM issues WHERE project_id = $4), \
         $5, $6, $4, $7, now(), now())",
    )
    .bind(issue_id)
    .bind(name)
    .bind(priority)
    .bind(project_id)
    .bind(state_id)
    .bind(type_id)
    .bind(sc.workspace_id)
    .execute(pool)
    .await
    .expect("scratch incident issue");
    issue_id
}

async fn link_issue_to_service(
    pool: &PgPool,
    sc: &Scratch,
    service_id: Uuid,
    project_id: Uuid,
    issue_id: Uuid,
) {
    sqlx::query(
        "INSERT INTO service_issues (id, workspace_id, project_id, service_id, issue_id, \
         created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now())",
    )
    .bind(sc.workspace_id)
    .bind(project_id)
    .bind(service_id)
    .bind(issue_id)
    .execute(pool)
    .await
    .expect("scratch service issue");
}

async fn link_service_to_war_room(
    pool: &PgPool,
    sc: &Scratch,
    war_room_id: Uuid,
    project_id: Uuid,
    service_id: Uuid,
) {
    sqlx::query(
        "INSERT INTO war_room_services (id, workspace_id, project_id, war_room_id, service_id, \
         created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now())",
    )
    .bind(sc.workspace_id)
    .bind(project_id)
    .bind(war_room_id)
    .bind(service_id)
    .execute(pool)
    .await
    .expect("scratch war room service");
}

async fn war_room_primary_issue(pool: &PgPool, war_room_id: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT primary_issue_id FROM war_rooms WHERE id = $1")
        .bind(war_room_id)
        .fetch_one(pool)
        .await
        .expect("scratch war room primary issue")
}

fn incident_snapshot<'a>(body: &'a serde_json::Value, service_id: Uuid) -> &'a serde_json::Value {
    let expected = service_id.to_string();
    body.as_array()
        .unwrap()
        .iter()
        .find(|row| row["service_id"].as_str() == Some(expected.as_str()))
        .expect("service snapshot")
}

#[tokio::test]
async fn health_derives_from_open_incidents_and_war_rooms() {
    use api::routes::service_health::workspace_health;
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    let open_issue = insert_incident(&st.pool, &sc, sc.p1, "DB latency", "high", "unstarted").await;
    link_issue_to_service(&st.pool, &sc, sc.s1, sc.p1, open_issue).await;

    let completed_issue =
        insert_incident(&st.pool, &sc, sc.p1, "Old outage", "none", "completed").await;
    link_issue_to_service(&st.pool, &sc, sc.s1, sc.p1, completed_issue).await;

    let room_id = insert_war_room(&st.pool, &sc, sc.p1).await;
    let room_issue = war_room_primary_issue(&st.pool, room_id).await;
    link_service_to_war_room(&st.pool, &sc, room_id, sc.p1, sc.s1).await;
    link_issue_to_service(&st.pool, &sc, sc.s1, sc.p1, room_issue).await;

    let p2_issue = insert_incident(&st.pool, &sc, sc.p2, "P2 incident", "none", "unstarted").await;
    link_issue_to_service(&st.pool, &sc, sc.s1, sc.p2, p2_issue).await;

    let (status, _, Json(body)) = workspace_health(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(
        body.as_array().unwrap().len(),
        1,
        "owner member sees only S1"
    );

    let s1 = incident_snapshot(&body, sc.s1);
    assert_eq!(s1["health"], "degraded");
    assert_eq!(s1["last_deployed_at"], serde_json::Value::Null);
    let incidents = s1["incidents"].as_array().unwrap();
    assert_eq!(
        incidents.len(),
        2,
        "completed + inaccessible P2 incidents excluded; war room deduped"
    );
    assert_eq!(incidents[0]["severity"], "sev2", "highest severity first");
    assert!(incidents[0]["identifier"]
        .as_str()
        .unwrap()
        .starts_with("P1-"));

    let room_id_string = room_id.to_string();
    let room_incident = incidents
        .iter()
        .find(|incident| incident["war_room_id"].as_str() == Some(room_id_string.as_str()))
        .expect("war room incident");
    assert_eq!(room_incident["war_room_sequence_id"].as_i64(), Some(1));
    assert_eq!(room_incident["severity"], "sev3");
    assert_eq!(room_incident["name"], "War room primary");

    let (_, _, Json(admin_body)) =
        workspace_health(State(st.clone()), AuthUser(sc.admin), Path(sc.slug.clone()))
            .await
            .unwrap();
    let admin_s1 = incident_snapshot(&admin_body, sc.s1);
    assert_eq!(
        admin_s1["incidents"].as_array().unwrap().len(),
        3,
        "workspace admin sees the P2 incident too"
    );
}

#[tokio::test]
async fn health_respects_project_scope_and_lifecycle() {
    use api::routes::service_health::{project_health, workspace_health};
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    let issue = insert_incident(&st.pool, &sc, sc.p1, "API down", "none", "unstarted").await;
    link_issue_to_service(&st.pool, &sc, sc.s1, sc.p1, issue).await;

    let denied = project_health(
        State(st.clone()),
        AuthUser(sc.outsider),
        Path((sc.slug.clone(), sc.p2)),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, axum::http::StatusCode::FORBIDDEN);

    let (_, _, Json(body)) = project_health(
        State(st.clone()),
        AuthUser(sc.consumer_admin),
        Path((sc.slug.clone(), sc.p2)),
    )
    .await
    .unwrap();
    assert_eq!(body.as_array().unwrap().len(), 2, "S1 consumer + S2 owner");
    assert_eq!(incident_snapshot(&body, sc.s1)["health"], "healthy");
    assert_eq!(incident_snapshot(&body, sc.s2)["health"], "healthy");

    let (_, _, Json(owner_body)) = project_health(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1)),
    )
    .await
    .unwrap();
    assert_eq!(incident_snapshot(&owner_body, sc.s1)["health"], "degraded");
    assert_eq!(
        incident_snapshot(&owner_body, sc.s1)["incidents"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    sqlx::query("UPDATE services SET status = 'planned' WHERE id = $1")
        .bind(sc.s1)
        .execute(&st.pool)
        .await
        .unwrap();
    let (_, _, Json(planned_body)) = workspace_health(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
    )
    .await
    .unwrap();
    let planned = incident_snapshot(&planned_body, sc.s1);
    assert_eq!(planned["health"], "unknown");
    assert_eq!(planned["incidents"].as_array().unwrap().len(), 1);

    sqlx::query("UPDATE issues SET archived_at = CURRENT_DATE WHERE id = $1")
        .bind(issue)
        .execute(&st.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE services SET status = 'active' WHERE id = $1")
        .bind(sc.s1)
        .execute(&st.pool)
        .await
        .unwrap();
    let (_, _, Json(archived_body)) = workspace_health(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
    )
    .await
    .unwrap();
    assert_eq!(
        incident_snapshot(&archived_body, sc.s1)["health"],
        "healthy"
    );

    sqlx::query("UPDATE issues SET archived_at = NULL, state_id = NULL WHERE id = $1")
        .bind(issue)
        .execute(&st.pool)
        .await
        .unwrap();
    let (_, _, Json(no_state_body)) = workspace_health(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path(sc.slug.clone()),
    )
    .await
    .unwrap();
    assert_eq!(
        incident_snapshot(&no_state_body, sc.s1)["health"],
        "degraded"
    );
}

#[tokio::test]
async fn war_room_attach_requires_linked_service() {
    use api::routes::war_room::{services_create, LinkServices};
    use axum::extract::{Path, State};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;
    let room_id = insert_war_room(&st.pool, &sc, sc.p1).await;

    // S2 is not linked to P1 -> rejected
    let denied = services_create(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1, room_id)),
        Json(LinkServices {
            service_ids: vec![sc.s2],
        }),
    )
    .await
    .unwrap();
    assert_eq!(denied.0, axum::http::StatusCode::BAD_REQUEST);

    // S1 is linked -> allowed
    let (status, _) = services_create(
        State(st.clone()),
        AuthUser(sc.owner_member),
        Path((sc.slug.clone(), sc.p1, room_id)),
        Json(LinkServices {
            service_ids: vec![sc.s1],
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);

    // S1 (consumer-linked to P2) can be attached to a P2 war room
    let p2_room = insert_war_room(&st.pool, &sc, sc.p2).await;
    let (status, _) = services_create(
        State(st.clone()),
        AuthUser(sc.consumer_admin),
        Path((sc.slug.clone(), sc.p2, p2_room)),
        Json(LinkServices {
            service_ids: vec![sc.s1],
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        status,
        axum::http::StatusCode::CREATED,
        "consumer-linked service is attachable by the consumer project"
    );
}

#[tokio::test]
async fn intake_accept_gate_requires_linked_service() {
    let st = state().await;
    let sc = Scratch::new(&st.pool).await;
    let issue_id = insert_issue(&st.pool, &sc, sc.p1, "Intake item").await;

    // service linked to another project only -> gate message
    sqlx::query(
        "INSERT INTO service_issues (id, workspace_id, project_id, service_id, issue_id, \
         created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now())",
    )
    .bind(sc.workspace_id)
    .bind(sc.p1)
    .bind(sc.s2)
    .bind(issue_id)
    .execute(&st.pool)
    .await
    .unwrap();

    let gate = api::routes::intake::accept_gate_for_test(&st.pool, sc.p1, issue_id)
        .await
        .unwrap();
    assert_eq!(
        gate,
        Some("Select a service before accepting this work item")
    );

    // linked service -> gate passes
    sqlx::query(
        "INSERT INTO service_issues (id, workspace_id, project_id, service_id, issue_id, \
         created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now())",
    )
    .bind(sc.workspace_id)
    .bind(sc.p1)
    .bind(sc.s1)
    .bind(issue_id)
    .execute(&st.pool)
    .await
    .unwrap();
    let gate = api::routes::intake::accept_gate_for_test(&st.pool, sc.p1, issue_id)
        .await
        .unwrap();
    assert_eq!(gate, None);
}

#[tokio::test]
async fn ai_list_services_respects_visibility() {
    use ai::agent::new_trace;
    use ai::tools::services::{ListServices, ListServicesArgs};
    use rig::tool::{Tool, ToolContext};

    let st = state().await;
    let sc = Scratch::new(&st.pool).await;

    let args = |query: Option<&str>| ListServicesArgs {
        query: query.map(str::to_string),
        status: None,
        criticality: None,
        service_type: None,
        project: None,
        limit: None,
    };

    // P1 member: only S1
    let tool = ListServices {
        pool: st.pool.clone(),
        workspace_id: sc.workspace_id,
        user_id: sc.owner_member,
        trace: new_trace(),
    };
    let out = tool
        .call(&mut ToolContext::new(), args(None))
        .await
        .unwrap();
    assert!(out.contains("ISO Switch"));
    assert!(!out.contains("Core Banking"), "S2 is not linked to P1");
    assert!(!out.contains("P4 Widget"));

    // P2 admin sees S1 (consumer) + S2 (owner)
    let tool = ListServices {
        pool: st.pool.clone(),
        workspace_id: sc.workspace_id,
        user_id: sc.consumer_admin,
        trace: new_trace(),
    };
    let out = tool
        .call(&mut ToolContext::new(), args(None))
        .await
        .unwrap();
    assert!(out.contains("ISO Switch"));
    assert!(out.contains("Core Banking"));

    // ws admin sees all
    let tool = ListServices {
        pool: st.pool.clone(),
        workspace_id: sc.workspace_id,
        user_id: sc.admin,
        trace: new_trace(),
    };
    let out = tool
        .call(&mut ToolContext::new(), args(None))
        .await
        .unwrap();
    assert!(out.contains("P4 Widget"));
}

#[tokio::test]
async fn full_visible_rejects_inactive_membership() {
    use api::routes::service_access::full_visible;

    let pool = pool().await;
    let sc = Scratch::new(&pool).await;

    sqlx::query(
        "UPDATE project_members SET is_active = false WHERE member_id = $1 AND project_id = $2",
    )
    .bind(sc.owner_member)
    .bind(sc.p1)
    .execute(&pool)
    .await
    .unwrap();

    assert!(
        !full_visible(&pool, sc.owner_member, &sc.slug, sc.s1)
            .await
            .unwrap(),
        "inactive project membership grants no visibility"
    );
}
