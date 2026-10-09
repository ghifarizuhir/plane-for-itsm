//! War room backend integration tests (Phase 1): create defaults/sequence,
//! duplicate guard, transitions, links, participants, runbook, events,
//! access gates, list/summary. DB-backed; run serially:
//! `DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
//!  cargo test -p api --test war_room_test -- --test-threads=1`

use api::middleware::auth::AuthUser;
use api::routes::issue_query::{list as list_issues, ProjectIssuesQuery};
use api::routes::war_room::{
    create, destroy, detail, events_list, issues_create, issues_destroy, list, messages_create,
    messages_destroy, messages_list, messages_patch, participants_create, participants_destroy,
    participants_patch, patch, runbook_create, runbook_patch, services_create, services_destroy,
    summary, CreateWarRoom, EventsParams, LinkIssues, LinkServices, ListParams, MessageCreate,
    MessagePatch, MessagesParams, ParticipantCreate, ParticipantPatch, PatchWarRoom, RunbookCreate,
    RunbookPatch,
};
use api::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use common::config::AppConfig;
use futures::StreamExt;
use serde_json::Value;
use sqlx::PgPool;
use std::time::Duration;
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
        let slug = format!("wr-{}", Uuid::new_v4().simple());
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let state_id = Uuid::new_v4();
        let type_id = Uuid::new_v4();
        let identifier = format!("WR{}", &Uuid::new_v4().simple().to_string()[..8]).to_uppercase();

        insert_user(pool, user_id, &slug).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'War Room Scratch', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
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
             VALUES ($1, now(), now(), 'War Room Scratch', '', 2, $2, $3, false, false, false, \
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
             VALUES ($1, 'Investigating', '', '#F59E0B', 'investigating', $2, $3, 65535, \
             'started', true, false, now(), now())",
        )
        .bind(state_id)
        .bind(project_id)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch state");
        sqlx::query(
            "INSERT INTO issue_types (id, name, description, logo_props, workspace_id, is_active, \
             is_default, level, is_epic, requires_service, created_at, updated_at) \
             VALUES ($1, 'Incident', '', '{}'::jsonb, $2, true, true, 0, false, false, now(), now())",
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

    async fn insert_issue(&self, pool: &PgPool, assignee: Option<Uuid>) -> Uuid {
        // Both rows in one transaction: the DB-wide `issue_sequences`
        // invariant (checked by a parallel test binary) must never observe
        // the issue without its counter row.
        let mut tx = pool.begin().await.expect("scratch tx");
        let (issue_id, sequence_id): (Uuid, i32) = sqlx::query_as(
            "INSERT INTO issues (id, name, description_html, description_json, priority, is_draft, \
             sort_order, sequence_id, state_id, type_id, project_id, workspace_id, created_at, updated_at) \
             VALUES (gen_random_uuid(), 'QRIS timeout massal', '<p></p>', '{}', 'urgent', false, \
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
        // Keep the DB-wide `issue_sequences` invariant (checked by
        // `issue_create_test::every_active_issue_has_a_matching_sequence_row`,
        // which runs in a parallel test binary): every active issue needs its
        // counter row.
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
        if let Some(user_id) = assignee {
            sqlx::query(
                "INSERT INTO issue_assignees (id, assignee_id, issue_id, project_id, workspace_id, \
                 created_at, updated_at) \
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now())",
            )
            .bind(user_id)
            .bind(issue_id)
            .bind(self.project_id)
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .expect("scratch issue assignee");
        }
        issue_id
    }

    async fn insert_service(&self, pool: &PgPool, name: &str) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO services (id, workspace_id, project_id, name, description, \
             description_html, status, criticality, \"type\", sort_order, created_at, updated_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, '', '', 'active', 'high', 'internal', 65535, \
             now(), now()) RETURNING id",
        )
        .bind(self.workspace_id)
        .bind(self.project_id)
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("scratch service")
    }

    async fn cleanup(&self, pool: &PgPool) {
        // `notifications` FKs are NO ACTION and mention rows reference the
        // scratch workspace/users, so clear them before the workspace delete.
        sqlx::query("DELETE FROM notifications WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
        // No cascade on issue_sequences: delete counter rows first so the
        // project delete below is not blocked by its FK.
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

#[tokio::test]
async fn create_seeds_defaults_runbook_and_sequence() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let assignee = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    let issue_id = scratch.insert_issue(&st.pool, Some(assignee)).await;
    let service_id = scratch.insert_service(&st.pool, "Payment Gateway").await;

    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(CreateWarRoom {
            name: None,
            primary_issue_id: issue_id,
            severity: None,
            description_html: None,
            service_ids: Some(vec![service_id]),
        }),
    )
    .await
    .expect("create room");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["sequence_id"], 1);
    assert_eq!(body["name"], "QRIS timeout massal");
    assert_eq!(body["severity"], "sev1");
    assert_eq!(body["status"], "active");
    assert!(body["primary_issue"]["identifier"]
        .as_str()
        .unwrap()
        .ends_with("-1"));
    assert_eq!(body["services"].as_array().unwrap().len(), 1);
    assert_eq!(body["participants"].as_array().unwrap().len(), 2);
    assert_eq!(body["runbook_items"].as_array().unwrap().len(), 5);
    assert_eq!(body["counts"]["messages"], 0);

    let other_issue = scratch.insert_issue(&st.pool, None).await;
    let (status, Json(second)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(CreateWarRoom {
            name: Some("Second room".into()),
            primary_issue_id: other_issue,
            severity: Some("sev3".into()),
            description_html: None,
            service_ids: None,
        }),
    )
    .await
    .expect("create second room");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(second["sequence_id"], 2);
    assert_eq!(second["runbook_items"].as_array().unwrap().len(), 5);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn create_conflicts_when_active_room_exists() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let payload = || CreateWarRoom {
        name: None,
        primary_issue_id: issue_id,
        severity: None,
        description_html: None,
        service_ids: None,
    };
    let (status, _) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(payload()),
    )
    .await
    .expect("first create");
    assert_eq!(status, StatusCode::CREATED);

    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(payload()),
    )
    .await
    .expect("duplicate create");
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "active_war_room_exists");
    assert!(body["war_room_id"].is_string());

    scratch.cleanup(&st.pool).await;
}

async fn create_room(st: &AppState, scratch: &Scratch, issue_id: Uuid) -> Value {
    let (status, Json(body)) = create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Json(CreateWarRoom {
            name: None,
            primary_issue_id: issue_id,
            severity: None,
            description_html: None,
            service_ids: None,
        }),
    )
    .await
    .expect("create room");
    assert_eq!(status, StatusCode::CREATED);
    body
}

fn room_id(body: &Value) -> Uuid {
    Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn issue_list_exposes_and_filters_war_room_property() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;

    let query = |filters: &str| ProjectIssuesQuery {
        filters: Some(filters.to_string()),
        ..Default::default()
    };

    // Tanpa room: kolom turunan null dan filter `off` match.
    let (status, Json(body)) = list_issues(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Query(query(r#"{"and":[{"war_room__in":"off"}]}"#)),
    )
    .await
    .expect("list off");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_count"], 1);
    let row = &body["results"][0];
    assert!(row["war_room_id"].is_null());
    assert!(row["war_room_status"].is_null());
    assert!(row["war_room_severity"].is_null());

    create_room(&st, &scratch, issue_id).await;

    // Ada room active: filter `on` match dan row membawa id/status/severity.
    let (_, Json(body)) = list_issues(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Query(query(r#"{"and":[{"war_room__in":"on"}]}"#)),
    )
    .await
    .expect("list on");
    assert_eq!(body["total_count"], 1);
    let row = &body["results"][0];
    assert_eq!(row["war_room_status"], "active");
    assert!(row["war_room_id"].is_string());
    assert!(row["war_room_severity"].is_string());

    let (_, Json(body)) = list_issues(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Query(query(r#"{"and":[{"war_room__in":"off"}]}"#)),
    )
    .await
    .expect("list off again");
    assert_eq!(body["total_count"], 0);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn status_transitions_freeze_and_reopen() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);

    let (status, Json(body)) = patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(PatchWarRoom {
            name: None,
            severity: Some("sev2".into()),
            status: Some("resolved".into()),
            description_html: None,
            notes_html: Some("<p>Rollback done</p>".into()),
        }),
    )
    .await
    .expect("resolve");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "resolved");
    assert_eq!(body["severity"], "sev2");
    assert!(body["resolved_at"].is_string());

    let (status, Json(body)) = patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(PatchWarRoom {
            name: None,
            severity: None,
            status: Some("monitoring".into()),
            description_html: None,
            notes_html: None,
        }),
    )
    .await
    .expect("invalid transition");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_status_transition");

    let (status, Json(body)) = patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(PatchWarRoom {
            name: None,
            severity: None,
            status: Some("active".into()),
            description_html: None,
            notes_html: None,
        }),
    )
    .await
    .expect("reopen");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "active");
    assert!(body["resolved_at"].is_null());

    let (status, _) = patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(PatchWarRoom {
            name: None,
            severity: None,
            status: Some("archived".into()),
            description_html: None,
            notes_html: None,
        }),
    )
    .await
    .expect("archive");
    assert_eq!(status, StatusCode::OK);

    let (status, Json(body)) = patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(PatchWarRoom {
            name: None,
            severity: None,
            status: Some("active".into()),
            description_html: None,
            notes_html: None,
        }),
    )
    .await
    .expect("archived terminal");
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "room_archived");

    let (status, Json(body)) = runbook_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(RunbookCreate {
            title: "No write when archived".into(),
        }),
    )
    .await
    .expect("archived write");
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "room_archived");

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn links_are_idempotent_and_primary_is_rejected() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let other_issue = scratch.insert_issue(&st.pool, None).await;
    let service_id = scratch.insert_service(&st.pool, "QRIS Processor").await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);

    let (status, Json(body)) = services_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(LinkServices {
            service_ids: vec![service_id],
        }),
    )
    .await
    .expect("link service");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["linked"], 1);

    let (_, Json(body)) = services_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(LinkServices {
            service_ids: vec![service_id],
        }),
    )
    .await
    .expect("relink service");
    assert_eq!(body["linked"], 0);

    let (status, Json(body)) = issues_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(LinkIssues {
            issue_ids: vec![issue_id],
        }),
    )
    .await
    .expect("link primary");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "primary_issue_not_linkable");

    let (status, Json(body)) = issues_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(LinkIssues {
            issue_ids: vec![other_issue],
        }),
    )
    .await
    .expect("link other");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["linked"], 1);

    let (status, Json(body)) = detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
    )
    .await
    .expect("detail");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["issues"].as_array().unwrap().len(), 1);

    let (status, _) = issues_destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk, other_issue)),
    )
    .await
    .expect("unlink");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = services_destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk, service_id)),
    )
    .await
    .expect("unlink service");
    assert_eq!(status, StatusCode::NO_CONTENT);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn participants_keep_single_commander() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let teammate = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);

    let (status, Json(body)) = participants_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(ParticipantCreate {
            member_id: teammate,
            role: None,
        }),
    )
    .await
    .expect("add participant");
    assert_eq!(status, StatusCode::CREATED);
    let participant_id = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
    assert_eq!(body["role"], "responder");

    let (status, Json(body)) = participants_patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk, participant_id)),
        Json(ParticipantPatch {
            role: "commander".into(),
        }),
    )
    .await
    .expect("promote commander");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["role"], "commander");

    let (_, Json(body)) = detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
    )
    .await
    .expect("detail");
    let roles: Vec<&str> = body["participants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles.iter().filter(|r| **r == "commander").count(), 1);

    let (status, _) = participants_destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk, participant_id)),
    )
    .await
    .expect("leave");
    assert_eq!(status, StatusCode::NO_CONTENT);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn runbook_toggle_records_events() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);
    let item_id = Uuid::parse_str(room["runbook_items"][0]["id"].as_str().unwrap()).unwrap();

    let (status, Json(body)) = runbook_patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk, item_id)),
        Json(RunbookPatch {
            title: None,
            is_done: Some(true),
        }),
    )
    .await
    .expect("mark done");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["is_done"], true);
    assert!(body["done_by_id"].is_string());

    let (status, Json(body)) = runbook_patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk, item_id)),
        Json(RunbookPatch {
            title: None,
            is_done: Some(false),
        }),
    )
    .await
    .expect("reopen item");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["is_done"], false);
    assert!(body["done_by_id"].is_null());

    let (status, Json(events)) = events_list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Query(EventsParams {
            before_id: None,
            limit: None,
        }),
    )
    .await
    .expect("events");
    assert_eq!(status, StatusCode::OK);
    let types: Vec<&str> = events
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["event_type"].as_str().unwrap())
        .collect();
    assert!(types.contains(&"runbook.item_done"));
    assert!(types.contains(&"runbook.item_reopened"));
    assert!(types.contains(&"room.created"));

    let (status, Json(body)) = runbook_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(RunbookCreate {
            title: "Custom check".into(),
        }),
    )
    .await
    .expect("add item");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["title"], "Custom check");
    assert!(body["template_key"].is_null());

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn access_gates_deny_guest_writes_and_non_members() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let guest = scratch.add_actor(&st.pool, Some(5), Some(5)).await;
    let outsider = scratch.add_actor(&st.pool, None, None).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);

    let (status, _) = detail(
        State(st.clone()),
        AuthUser(guest),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
    )
    .await
    .expect("guest read");
    assert_eq!(status, StatusCode::OK);

    let (status, Json(body)) = patch(
        State(st.clone()),
        AuthUser(guest),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(PatchWarRoom {
            name: None,
            severity: None,
            status: Some("monitoring".into()),
            description_html: None,
            notes_html: None,
        }),
    )
    .await
    .expect("guest write");
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body["error"].is_string());

    let (status, _) = detail(
        State(st.clone()),
        AuthUser(outsider),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
    )
    .await
    .expect("outsider read");
    assert_eq!(status, StatusCode::FORBIDDEN);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn list_filters_and_summary_counts() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_a = scratch.insert_issue(&st.pool, None).await;
    let issue_b = scratch.insert_issue(&st.pool, None).await;
    let room_a = create_room(&st, &scratch, issue_a).await;
    let pk_a = room_id(&room_a);
    let _room_b = create_room(&st, &scratch, issue_b).await;

    let (status, _) = patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk_a)),
        Json(PatchWarRoom {
            name: None,
            severity: None,
            status: Some("resolved".into()),
            description_html: None,
            notes_html: None,
        }),
    )
    .await
    .expect("resolve room a");
    assert_eq!(status, StatusCode::OK);

    let (status, Json(body)) = list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Query(ListParams {
            status: Some("active,monitoring".into()),
            severity: None,
            q: None,
        }),
    )
    .await
    .expect("list active");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["sequence_id"], 2);

    let (status, Json(body)) = list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Query(ListParams {
            status: None,
            severity: None,
            q: Some("WR".into()),
        }),
    )
    .await
    .expect("list by incident identifier");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 2);

    let (status, Json(body)) = summary(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
    )
    .await
    .expect("summary");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active"], 1);
    assert_eq!(body["sev1_2"], 1);
    assert_eq!(body["resolved_7d"], 1);

    let (status, _) = list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id)),
        Query(ListParams {
            status: Some("bogus".into()),
            severity: None,
            q: None,
        }),
    )
    .await
    .expect("invalid status filter");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk_a)),
    )
    .await
    .expect("destroy");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk_a)),
    )
    .await
    .expect("detail after destroy");
    assert_eq!(status, StatusCode::NOT_FOUND);

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn messages_cursor_pagination_returns_chronological_pages() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);

    for body in ["first", "second", "third"] {
        let (status, _) = messages_create(
            State(st.clone()),
            AuthUser(scratch.user_id),
            Path((scratch.slug.clone(), scratch.project_id, pk)),
            Json(MessageCreate { body: body.into(), client_id: None }),
        )
        .await
        .expect("create message");
        assert_eq!(status, StatusCode::CREATED);
        // Distinct transaction timestamps keep the cursor ordering deterministic.
        tokio::time::sleep(Duration::from_millis(2)).await;
    }

    let (status, Json(page)) = messages_list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Query(MessagesParams { before_id: None, limit: Some(2) }),
    )
    .await
    .expect("list page");
    assert_eq!(status, StatusCode::OK);
    let bodies: Vec<&str> = page
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["body"].as_str().unwrap())
        .collect();
    assert_eq!(bodies, vec!["second", "third"]);

    let oldest_id = Uuid::parse_str(page[0]["id"].as_str().unwrap()).unwrap();
    let (status, Json(older)) = messages_list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Query(MessagesParams { before_id: Some(oldest_id), limit: Some(2) }),
    )
    .await
    .expect("list older");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(older.as_array().unwrap().len(), 1);
    assert_eq!(older[0]["body"], "first");

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn message_mentions_notify_workspace_members_only() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let teammate = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    let outsider = scratch.add_actor(&st.pool, None, None).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);

    let body = format!("halo @{{{teammate}}} @{{{outsider}}} @{{{}}}", scratch.user_id);
    let (status, Json(message)) = messages_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(MessageCreate { body, client_id: Some("client-1".into()) }),
    )
    .await
    .expect("create mention");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(message["client_id"], "client-1");
    let mentions: Vec<&str> = message["mentions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap())
        .collect();
    assert_eq!(mentions.len(), 2);
    assert!(mentions.contains(&teammate.to_string().as_str()));
    assert!(mentions.contains(&scratch.user_id.to_string().as_str()));

    let notified: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE entity_name = 'war_room' \
         AND entity_identifier = $1 AND receiver_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(teammate)
    .fetch_one(&st.pool)
    .await
    .expect("count notifications");
    assert_eq!(notified, 1);

    let author_notified: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE entity_name = 'war_room' \
         AND entity_identifier = $1 AND receiver_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(scratch.user_id)
    .fetch_one(&st.pool)
    .await
    .expect("count author notifications");
    assert_eq!(author_notified, 0);

    let (status, Json(body)) = messages_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(MessageCreate { body: "   ".into(), client_id: None }),
    )
    .await
    .expect("empty body");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].is_string());

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn message_edit_and_delete_are_author_only() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let teammate = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);

    let (_, Json(message)) = messages_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(MessageCreate { body: "original".into(), client_id: None }),
    )
    .await
    .expect("create");
    let message_id = Uuid::parse_str(message["id"].as_str().unwrap()).unwrap();

    let (status, _) = messages_patch(
        State(st.clone()),
        AuthUser(teammate),
        Path((scratch.slug.clone(), scratch.project_id, pk, message_id)),
        Json(MessagePatch { body: "hijack".into() }),
    )
    .await
    .expect("teammate edit");
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = messages_destroy(
        State(st.clone()),
        AuthUser(teammate),
        Path((scratch.slug.clone(), scratch.project_id, pk, message_id)),
    )
    .await
    .expect("teammate delete");
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, Json(edited)) = messages_patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk, message_id)),
        Json(MessagePatch { body: "edited".into() }),
    )
    .await
    .expect("author edit");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(edited["body"], "edited");
    assert!(edited["edited_at"].is_string());

    let (status, _) = messages_destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk, message_id)),
    )
    .await
    .expect("author delete");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, Json(page)) = messages_list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Query(MessagesParams { before_id: None, limit: None }),
    )
    .await
    .expect("list after delete");
    assert!(page.as_array().unwrap().is_empty());

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn archived_room_rejects_message_writes() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);

    let (status, _) = patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(PatchWarRoom {
            name: None,
            severity: None,
            status: Some("archived".into()),
            description_html: None,
            notes_html: None,
        }),
    )
    .await
    .expect("archive");
    assert_eq!(status, StatusCode::OK);

    let (status, Json(body)) = messages_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(MessageCreate { body: "too late".into(), client_id: None }),
    )
    .await
    .expect("archived message");
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "room_archived");

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn message_create_publishes_realtime_event() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    let client = redis::Client::open("redis://127.0.0.1:6379").expect("redis client");
    let Ok(conn) = client.get_async_connection().await else {
        eprintln!("redis unavailable, skipping publish assertion");
        scratch.cleanup(&st.pool).await;
        return;
    };
    let mut pubsub = conn.into_pubsub();
    pubsub
        .subscribe("war-room:events")
        .await
        .expect("subscribe");
    let mut stream = pubsub.on_message();

    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);

    let (_, Json(message)) = messages_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(MessageCreate { body: "ping room".into(), client_id: Some("client-9".into()) }),
    )
    .await
    .expect("create message");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let Ok(Some(msg)) = tokio::time::timeout(remaining, stream.next()).await else {
            panic!("no message.created event published before deadline");
        };
        let payload: String = msg.get_payload().expect("payload");
        let value: Value = serde_json::from_str(&payload).expect("json payload");
        if value["kind"] == "message.created" {
            assert_eq!(value["room_id"], pk.to_string());
            assert_eq!(value["data"]["id"], message["id"]);
            assert_eq!(value["data"]["body"], "ping room");
            assert_eq!(value["data"]["client_id"], "client-9");
            break;
        }
    }

    scratch.cleanup(&st.pool).await;
}
