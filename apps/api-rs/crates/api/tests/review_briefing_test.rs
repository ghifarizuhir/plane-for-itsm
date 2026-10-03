//! Review briefing (AI) integration tests: fake OpenAI-compatible upstream,
//! degradation paths, permissions. DB-backed and mutates process-level LLM env
//! → run serially as its own binary:
//! `DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
//!  cargo test -p api --test review_briefing_test -- --test-threads=1`

#[path = "support/mod.rs"]
mod support;

use api::middleware::auth::AuthUser;
use api::routes::review::{
    complete_session, create_session, items_create, session_detail, submit_request, AddItems,
    CreateSession, SubmitRequest,
};
use api::routes::review_briefing::{generate_briefing, BriefingRequest};
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
        let slug = format!("rb-{}", Uuid::new_v4().simple());
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let state_id = Uuid::new_v4();
        let type_id = Uuid::new_v4();
        let identifier = format!("RB{}", &Uuid::new_v4().simple().to_string()[..8]).to_uppercase();

        insert_user(pool, user_id, &slug).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'Briefing Scratch', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
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
             VALUES ($1, now(), now(), 'Briefing Scratch', '', 2, $2, $3, false, false, false, \
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

    async fn add_actor(&mut self, pool: &PgPool, ws_role: i16) -> Uuid {
        let user_id = Uuid::new_v4();
        let username = format!("{}-{}", self.slug, &user_id.simple().to_string()[..8]);
        insert_user(pool, user_id, &username).await;
        insert_workspace_member(pool, user_id, self.workspace_id, ws_role).await;
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

fn set_llm_env(base_url: &str) {
    std::env::set_var("SKIP_ENV_VAR", "0");
    std::env::set_var("LLM_API_KEY", "test-key");
    std::env::set_var("LLM_BASE_URL", base_url);
    std::env::set_var("LLM_MODEL", "test-model");
}

fn clear_llm_env() {
    std::env::remove_var("SKIP_ENV_VAR");
    std::env::remove_var("LLM_API_KEY");
    std::env::remove_var("LLM_BASE_URL");
    std::env::remove_var("LLM_MODEL");
}

async fn spawn_status(status: u16) -> String {
    use axum::routing::post;
    let handler = move || async move {
        (
            StatusCode::from_u16(status).unwrap(),
            Json(json!({"error": "boom"})),
        )
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new().route("/v1/chat/completions", post(handler)),
        )
        .await
        .unwrap();
    });
    format!("http://{addr}/v1")
}

fn submit_body(board: &str, change: Option<Uuid>, release: Option<Uuid>) -> SubmitRequest {
    SubmitRequest {
        board_type: Some(board.to_string()),
        change_issue_id: change,
        release_id: release,
        submission_note: Some("Bukti uji terlampir".to_string()),
    }
}

fn session_body(board: &str, project_id: Option<Uuid>, title: &str) -> CreateSession {
    CreateSession {
        board_type: Some(board.to_string()),
        project_id,
        title: Some(title.to_string()),
        scheduled_at: Some(
            chrono::DateTime::parse_from_rfc3339("2026-10-10T09:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        ),
        location: Some("Ruang Rapat 3".to_string()),
        minutes: None,
    }
}

fn add_items(request_id: Uuid) -> AddItems {
    AddItems {
        request_ids: vec![request_id],
    }
}

fn request_id_of(body: &Value) -> Uuid {
    body["id"].as_str().unwrap().parse().unwrap()
}

/// TCB session with one submitted change on the agenda.
/// Returns `(request_id, session_id, item_id)`.
async fn setup_session(st: &AppState, scratch: &Scratch) -> (Uuid, Uuid, Uuid) {
    let issue_id = scratch.insert_issue(&st.pool).await;
    let (_, Json(request)) = submit_request(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(submit_body("tcb", Some(issue_id), None)),
    )
    .await
    .expect("submit");
    let request_id = request_id_of(&request);
    let (_, Json(session)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("tcb", Some(scratch.project_id), "TCB Briefing")),
    )
    .await
    .expect("create session");
    let session_id: Uuid = session["id"].as_str().unwrap().parse().unwrap();
    let (_, Json(items)) = items_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(add_items(request_id)),
    )
    .await
    .expect("add items");
    let item_id: Uuid = items[0]["id"].as_str().unwrap().parse().unwrap();
    (request_id, session_id, item_id)
}

#[tokio::test]
async fn generate_briefing_stores_json_and_regenerates() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let (_request_id, session_id, item_id) = setup_session(&st, &scratch).await;

    let answer = json!({
        "overall": "Satu item menunggu keputusan.",
        "items": [{
            "session_item_id": item_id,
            "summary": "Perubahan retry gateway.",
            "discussion_points": ["Pantau setelah deploy"],
            "risks": ["Menyentuh gateway pembayaran"]
        }]
    })
    .to_string();
    let (base, bodies) = support::spawn_recording_upstream(&answer).await;
    set_llm_env(&base);

    let (status, Json(briefing)) = generate_briefing(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(BriefingRequest {
            language: Some("id".into()),
        }),
    )
    .await
    .expect("generate");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(briefing["format"], "json");
    assert_eq!(briefing["language"], "id");
    assert_eq!(briefing["included_items"], 1);
    assert_eq!(briefing["skipped_items"], 0);
    assert_eq!(briefing["items"][0]["session_item_id"], item_id.to_string());
    assert_eq!(briefing["items"][0]["risks"][0], "Menyentuh gateway pembayaran");

    let request_body = bodies.lock().unwrap()[0].clone();
    let prompt = request_body["messages"][0]["content"].as_str().unwrap();
    assert!(prompt.contains("TCB Briefing"));
    assert!(prompt.contains(&item_id.to_string()));

    let (_, Json(detail)) = session_detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
    )
    .await
    .expect("detail");
    assert_eq!(detail["briefing"]["format"], "json");
    assert_eq!(detail["briefing"]["items"][0]["summary"], "Perubahan retry gateway.");

    let second = json!({"overall": "Versi kedua.", "items": []}).to_string();
    let (base2, _) = support::spawn_recording_upstream(&second).await;
    set_llm_env(&base2);
    let (_, Json(regenerated)) = generate_briefing(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(BriefingRequest { language: None }),
    )
    .await
    .expect("regenerate");
    assert_eq!(regenerated["overall"], "Versi kedua.");
    assert_eq!(regenerated["language"], "en");
    assert_eq!(regenerated["items"].as_array().unwrap().len(), 0);

    clear_llm_env();
    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn generate_briefing_requires_ai_config() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let (_request_id, session_id, _item_id) = setup_session(&st, &scratch).await;

    std::env::set_var("SKIP_ENV_VAR", "0");
    std::env::remove_var("LLM_API_KEY");
    std::env::set_var("LLM_MODEL", "test-model");
    let (status, Json(body)) = generate_briefing(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(BriefingRequest::default()),
    )
    .await
    .expect("not configured");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "AI is not configured for this workspace.");

    clear_llm_env();
    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn generate_briefing_gates_manage_and_scheduled() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let outsider = scratch.add_actor(&st.pool, 15).await;
    let (_request_id, session_id, _item_id) = setup_session(&st, &scratch).await;

    let (base, _) = support::spawn_recording_upstream(r#"{"overall":"ok","items":[]}"#).await;
    set_llm_env(&base);

    let (status, _) = generate_briefing(
        State(st.clone()),
        AuthUser(outsider),
        Path((scratch.slug.clone(), session_id)),
        Json(BriefingRequest::default()),
    )
    .await
    .expect("deny");
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (_, Json(_)) = complete_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
    )
    .await
    .expect("complete");
    let (status, Json(body)) = generate_briefing(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(BriefingRequest::default()),
    )
    .await
    .expect("completed");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Session is not scheduled");

    clear_llm_env();
    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn generate_briefing_rejects_empty_agenda() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let (_, Json(session)) = create_session(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(session_body("tcb", Some(scratch.project_id), "Empty")),
    )
    .await
    .expect("create session");
    let session_id: Uuid = session["id"].as_str().unwrap().parse().unwrap();

    let (base, _) = support::spawn_recording_upstream(r#"{"overall":"ok","items":[]}"#).await;
    set_llm_env(&base);
    let (status, Json(body)) = generate_briefing(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(BriefingRequest::default()),
    )
    .await
    .expect("empty agenda");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "The agenda is empty");

    clear_llm_env();
    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn generate_briefing_falls_back_to_text_on_bad_json() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let (_request_id, session_id, _item_id) = setup_session(&st, &scratch).await;

    let (base, bodies) = support::spawn_recording_upstream("ini bukan json").await;
    set_llm_env(&base);
    let (status, Json(briefing)) = generate_briefing(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(BriefingRequest::default()),
    )
    .await
    .expect("fallback");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(briefing["format"], "text");
    assert_eq!(briefing["overall"], "ini bukan json");
    assert!(briefing["items"].as_array().unwrap().is_empty());
    assert_eq!(bodies.lock().unwrap().len(), 2, "must retry exactly once");

    clear_llm_env();
    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn generate_briefing_upstream_error_keeps_previous() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let (_request_id, session_id, _item_id) = setup_session(&st, &scratch).await;

    let (base, _) =
        support::spawn_recording_upstream(r#"{"overall":"Versi lama.","items":[]}"#).await;
    set_llm_env(&base);
    generate_briefing(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(BriefingRequest::default()),
    )
    .await
    .expect("first generate");

    let base500 = spawn_status(500).await;
    set_llm_env(&base500);
    let (status, _) = generate_briefing(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
        Json(BriefingRequest::default()),
    )
    .await
    .expect("upstream error");
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

    let (_, Json(detail)) = session_detail(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), session_id)),
    )
    .await
    .expect("detail");
    assert_eq!(detail["briefing"]["overall"], "Versi lama.");

    clear_llm_env();
    scratch.cleanup(&st.pool).await;
}
