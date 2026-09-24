//! Fake OpenAI-compatible upstream for `routes::ai_agent::run_agent`:
//! no-tool answer, tool round-trip, 429, 500, malformed JSON.
//!
//! The stateful `workspace_ai_agent` tests at the bottom need the dev DB and
//! mutate the process-level LLM env (`SKIP_ENV_VAR`, `LLM_*`), so this file
//! MUST run with `--test-threads=1`:
//! `cargo test -p api --test ai_agent_test -- --test-threads=1`.

use std::sync::{Arc, Mutex};

use api::middleware::auth::AuthUser;
use api::routes::ai_agent::run_agent;
use api::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use common::config::AppConfig;
use rig::tool::server::ToolServer;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

fn chat_response(content: &str) -> Value {
    json!({
        "id": "chatcmpl-1",
        "object": "chat.completion",
        "created": 0,
        "model": "test",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": content},
            "finish_reason": "stop"
        }]
    })
}

async fn spawn_fixed(status: u16, body: Value) -> String {
    let handler = move || {
        let body = body.clone();
        async move { (StatusCode::from_u16(status).unwrap(), Json(body)) }
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/v1/chat/completions", post(handler)),
        )
        .await
        .unwrap();
    });
    format!("http://{addr}/v1")
}

#[derive(Default)]
struct Capturing {
    bodies: Mutex<Vec<Value>>,
}

async fn spawn_capturing() -> (String, Arc<Capturing>) {
    async fn handler(
        State(state): State<Arc<Capturing>>,
        Json(body): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        state.bodies.lock().unwrap().push(body);
        (
            StatusCode::OK,
            Json(json!({
                "id": "chatcmpl-2",
                "object": "chat.completion",
                "created": 0,
                "model": "test",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "final answer"},
                    "finish_reason": "stop"
                }]
            })),
        )
    }
    let state: Arc<Capturing> = Arc::new(Capturing::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/v1/chat/completions", post(handler))
        .with_state(state.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}/v1"), state)
}

#[tokio::test]
async fn task_folds_into_upstream_user_message() {
    let (base, upstream) = spawn_capturing().await;
    let out = run_agent(
        &base,
        "key",
        "model",
        ToolServer::new().run(),
        Some("be terse"),
        "hi",
    )
    .await;
    assert_eq!(out, Ok("final answer".to_string()));
    let bodies = upstream.bodies.lock().unwrap();
    let messages = bodies[0]["messages"].as_array().unwrap();
    let user = messages
        .iter()
        .find(|m| m["role"] == json!("user"))
        .expect("upstream call must carry a user message");
    assert_eq!(user["content"], json!("be terse\nhi"));
}

#[tokio::test]
async fn prompt_without_tools_returns_content() {
    let base = spawn_fixed(200, chat_response("final answer")).await;
    let out = run_agent(
        &base,
        "key",
        "gpt-4o-mini",
        ToolServer::new().run(),
        None,
        "hi",
    )
    .await;
    assert_eq!(out, Ok("final answer".to_string()));
}

mod tool_roundtrip {
    use std::sync::{Arc, Mutex};

    use api::routes::ai::LlmError;
    use api::routes::ai_agent::{new_trace, run_agent, ToolCallTrace, ToolTrace};
    use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
    use rig::tool::server::ToolServer;
    use rig::tool::{Tool, ToolContext, ToolExecutionError};
    use serde_json::{json, Value};

    #[derive(serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
    struct EchoArgs {
        text: String,
    }

    struct FakeEcho {
        trace: ToolTrace,
    }

    impl Tool for FakeEcho {
        const NAME: &'static str = "echo";
        type Args = EchoArgs;
        type Output = String;
        type Error = ToolExecutionError;

        fn description(&self) -> String {
            "Echo the text back".to_string()
        }

        fn parameters(&self) -> Value {
            serde_json::to_value(schemars::schema_for!(EchoArgs)).unwrap()
        }

        async fn call(
            &self,
            _context: &mut ToolContext,
            args: Self::Args,
        ) -> Result<Self::Output, Self::Error> {
            self.trace.lock().unwrap().push(ToolCallTrace {
                name: Self::NAME.to_string(),
                arguments: serde_json::to_value(&args).unwrap(),
            });
            Ok(format!("echo:{}", args.text))
        }
    }

    #[derive(Default)]
    struct Upstream {
        calls: Mutex<usize>,
        bodies: Mutex<Vec<Value>>,
    }

    type Shared = Arc<Upstream>;

    async fn handler(
        State(state): State<Shared>,
        Json(body): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        let n = {
            let mut calls = state.calls.lock().unwrap();
            let n = *calls;
            *calls += 1;
            n
        };
        state.bodies.lock().unwrap().push(body);
        if n == 0 {
            (
                StatusCode::OK,
                Json(json!({
                    "id": "1", "object": "chat.completion", "created": 0, "model": "test",
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": null,
                            "tool_calls": [{
                                "id": "call_1",
                                "type": "function",
                                "function": {"name": "echo", "arguments": "{\"text\":\"hi\"}"}
                            }]
                        },
                        "finish_reason": "tool_calls"
                    }]
                })),
            )
        } else {
            (
                StatusCode::OK,
                Json(json!({
                    "id": "2", "object": "chat.completion", "created": 0, "model": "test",
                    "choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": "final answer"},
                        "finish_reason": "stop"
                    }]
                })),
            )
        }
    }

    async fn spawn_roundtrip() -> (String, Shared) {
        let state: Shared = Arc::new(Upstream::default());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/v1/chat/completions", post(handler))
            .with_state(state.clone());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}/v1"), state)
    }

    async fn schedule_handler(
        State(state): State<Shared>,
        Json(body): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        let n = {
            let mut calls = state.calls.lock().unwrap();
            let n = *calls;
            *calls += 1;
            n
        };
        state.bodies.lock().unwrap().push(body);
        if n == 0 {
            (
                StatusCode::OK,
                Json(json!({
                    "id": "1", "object": "chat.completion", "created": 0, "model": "test",
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": null,
                            "tool_calls": [{
                                "id": "call_1",
                                "type": "function",
                                "function": {
                                    "name": "create_schedule",
                                    "arguments": "{\"name\":\"Daily\",\"prompt\":\"Report\",\"frequency\":\"daily\",\"time\":\"09:00\",\"timezone\":\"UTC\"}"
                                }
                            }]
                        },
                        "finish_reason": "tool_calls"
                    }]
                })),
            )
        } else {
            (
                StatusCode::OK,
                Json(json!({
                    "id": "2", "object": "chat.completion", "created": 0, "model": "test",
                    "choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": "final answer"},
                        "finish_reason": "stop"
                    }]
                })),
            )
        }
    }

    async fn spawn_schedule_roundtrip() -> (String, Shared) {
        let state: Shared = Arc::new(Upstream::default());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/v1/chat/completions", post(schedule_handler))
            .with_state(state.clone());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}/v1"), state)
    }

    /// URL only — for DB-backed tests that don't need the upstream handle.
    pub(super) async fn schedule_roundtrip_url() -> String {
        spawn_schedule_roundtrip().await.0
    }

    #[tokio::test]
    async fn tool_call_roundtrip_records_trace_and_returns_final_text() {
        let (base, upstream) = spawn_roundtrip().await;
        let trace = new_trace();
        let tool = FakeEcho {
            trace: trace.clone(),
        };
        let out = run_agent(
            &base,
            "key",
            "model",
            ToolServer::new().tool(tool).run(),
            None,
            "say hi",
        )
        .await;
        assert_eq!(out, Ok("final answer".to_string()));

        let recorded = trace.lock().unwrap().clone();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].name, "echo");
        assert_eq!(recorded[0].arguments["text"], json!("hi"));

        let bodies = upstream.bodies.lock().unwrap();
        assert_eq!(
            bodies.len(),
            2,
            "tool loop must issue a second upstream call"
        );
        let messages = bodies[1]["messages"].as_array().unwrap();
        let tool_msg = messages
            .iter()
            .find(|m| m["role"] == json!("tool"))
            .expect("second call must carry the tool result message");
        assert_eq!(tool_msg["content"], json!("echo:hi"));
    }

    #[tokio::test]
    async fn create_schedule_roundtrip_surfaces_pending_action() {
        let (base, upstream) = spawn_schedule_roundtrip().await;
        let trace = new_trace();
        let tool = ai::tools::CreateSchedule {
            trace: trace.clone(),
        };
        let out = run_agent(
            &base,
            "key",
            "model",
            ToolServer::new().tool(tool).run(),
            None,
            "/schedule daily report",
        )
        .await;
        assert_eq!(out, Ok("final answer".to_string()));

        let action = api::routes::ai_agent::pending_action(&trace).expect("proposal");
        assert_eq!(action["kind"], json!("create_schedule"));
        assert_eq!(action["proposal"]["frequency"], json!("daily"));
        assert_eq!(action["proposal"]["time"], json!("09:00"));

        let bodies = upstream.bodies.lock().unwrap();
        assert_eq!(
            bodies.len(),
            2,
            "tool loop must issue a second upstream call"
        );
    }

    #[tokio::test]
    async fn upstream_429_maps_to_rate_limited() {
        let base = super::spawn_fixed(429, json!({"error": {"message": "slow down"}})).await;
        let out = run_agent(&base, "key", "model", ToolServer::new().run(), None, "hi").await;
        assert_eq!(out, Err(LlmError::RateLimited));
    }

    #[tokio::test]
    async fn upstream_500_maps_to_upstream() {
        let base = super::spawn_fixed(500, json!({"error": "boom"})).await;
        let out = run_agent(&base, "key", "model", ToolServer::new().run(), None, "hi").await;
        assert_eq!(out, Err(LlmError::Upstream));
    }

    #[tokio::test]
    async fn malformed_json_maps_to_upstream() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/v1/chat/completions",
            post(|| async {
                axum::response::Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from("not-json"))
                    .unwrap()
            }),
        );
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let base = format!("http://{addr}/v1");
        let out = run_agent(&base, "key", "model", ToolServer::new().run(), None, "hi").await;
        assert_eq!(out, Err(LlmError::Upstream));
    }
}

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
    user_id: Uuid,
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

impl Scratch {
    async fn new(pool: &PgPool) -> Self {
        let slug = format!("aia-{}", Uuid::new_v4().simple());
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        insert_user(pool, user_id, &slug).await;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, created_at, updated_at, timezone, \
             background_color) VALUES ($1, 'AI Agent', $2, $3, now(), now(), 'UTC', '#FFFFFF')",
        )
        .bind(workspace_id)
        .bind(&slug)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("scratch workspace");
        sqlx::query(
            "INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, \
             workspace_id, view_props, default_props, issue_props, explored_features, \
             getting_started_checklist, tips, is_active) \
             VALUES (gen_random_uuid(), now(), now(), 20, $1, $2, '{}', '{}', '{}', '{}', '{}', \
             '{}', true)",
        )
        .bind(user_id)
        .bind(workspace_id)
        .execute(pool)
        .await
        .expect("scratch workspace member");
        Self {
            slug,
            workspace_id,
            user_id,
        }
    }

    async fn add_actor(&self, pool: &PgPool, workspace_role: i16) -> Uuid {
        let user_id = Uuid::new_v4();
        let username = format!("{}-{}", self.slug, &user_id.simple().to_string()[..8]);
        insert_user(pool, user_id, &username).await;
        sqlx::query(
            "INSERT INTO workspace_members (id, created_at, updated_at, role, member_id, \
             workspace_id, view_props, default_props, issue_props, explored_features, \
             getting_started_checklist, tips, is_active) \
             VALUES (gen_random_uuid(), now(), now(), $1, $2, $3, '{}', '{}', '{}', '{}', '{}', \
             '{}', true)",
        )
        .bind(workspace_role)
        .bind(user_id)
        .bind(self.workspace_id)
        .execute(pool)
        .await
        .expect("scratch actor");
        user_id
    }

    async fn purge(&self, pool: &PgPool) {
        sqlx::query("DELETE FROM ai_conversations WHERE workspace_id = $1")
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
        sqlx::query("DELETE FROM users WHERE username LIKE $1")
            .bind(format!("{}%", self.slug))
            .execute(pool)
            .await
            .ok();
    }
}

/// Fake OpenAI-compatible upstream that records every request body.
async fn spawn_recording_upstream() -> (String, std::sync::Arc<std::sync::Mutex<Vec<Value>>>) {
    use axum::{routing::post, Json, Router};
    let bodies: std::sync::Arc<std::sync::Mutex<Vec<Value>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorder = bodies.clone();
    async fn handler(
        axum::extract::State(recorder): axum::extract::State<
            std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
        >,
        Json(body): Json<Value>,
    ) -> (axum::http::StatusCode, Json<Value>) {
        recorder.lock().unwrap().push(body);
        (
            axum::http::StatusCode::OK,
            Json(json!({
                "id": "chatcmpl-test",
                "object": "chat.completion",
                "created": 0,
                "model": "test",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "agent answer"},
                    "finish_reason": "stop"
                }]
            })),
        )
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/chat/completions", post(handler))
                .with_state(recorder),
        )
        .await
        .unwrap();
    });
    (format!("http://{addr}/v1"), bodies)
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

async fn create_conversation(st: &AppState, slug: &str, user_id: Uuid, mode: &str) -> Uuid {
    let (_, Json(created)) = api::routes::ai_conversations::create(
        State(st.clone()),
        AuthUser(user_id),
        Path(slug.to_string()),
        Json(json!({"mode": mode})),
    )
    .await
    .expect("create conversation");
    Uuid::parse_str(created["id"].as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn agent_turn_persists_both_messages_and_builds_context_from_history() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    // Seed 10 older messages so the 8-message window has something to drop.
    // Raw SQL because `insert_message` is `pub(crate)`.
    for index in 0..10 {
        sqlx::query(
            "INSERT INTO ai_messages (id, conversation_id, role, content, metadata, created_at) \
             VALUES ($1, $2, $3, $4, '{}'::jsonb, now() - make_interval(secs => $5))",
        )
        .bind(Uuid::new_v4())
        .bind(conversation_id)
        .bind(if index % 2 == 0 { "user" } else { "assistant" })
        .bind(format!("seed-{index}"))
        .bind(100 - index)
        .execute(&pool)
        .await
        .expect("seed");
    }

    let (base_url, bodies) = spawn_recording_upstream().await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "how many items?",
            "context": "Work item context:\nWork item: X",
            "conversation_id": conversation_id,
        })),
    )
    .await
    .expect("agent call");
    clear_llm_env();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["response"], json!("agent answer"));
    assert_eq!(body["conversation"]["id"], json!(conversation_id));
    assert_eq!(body["conversation"]["title"], json!("how many items?"));
    assert_eq!(body["user_message"]["role"], json!("user"));
    assert_eq!(body["user_message"]["content"], json!("how many items?"));
    assert_eq!(body["assistant_message"]["role"], json!("assistant"));
    assert_eq!(body["assistant_message"]["content"], json!("agent answer"));

    // The upstream saw the composed prompt: context + newest 8 + question.
    // Rig sends the agent preamble as messages[0], so find the user message
    // by role instead of by index.
    let sent = bodies.lock().unwrap().clone();
    let content = sent[0]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["role"] == json!("user"))
        .and_then(|message| message["content"].as_str())
        .unwrap();
    assert!(content.contains("Work item context:"));
    assert!(content.contains("seed-2"), "oldest two of ten are dropped");
    assert!(!content.contains("seed-0"));
    assert!(!content.contains("seed-1"));
    assert!(content.contains("User's new question: how many items?"));
    assert_eq!(
        content.matches("how many items?").count(),
        1,
        "history is loaded before the user insert, so the question appears once"
    );

    // DB: user + assistant rows persisted, conversation title filled.
    let stored: Vec<(String, String)> = sqlx::query_as(
        "SELECT role, content FROM ai_messages WHERE conversation_id = $1 \
         ORDER BY created_at, id",
    )
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(stored.len(), 12);
    assert_eq!(stored[10].0, "user");
    assert_eq!(stored[10].1, "how many items?");
    assert_eq!(stored[11].0, "assistant");
    assert_eq!(stored[11].1, "agent answer");
    let title: String = sqlx::query_scalar("SELECT title FROM ai_conversations WHERE id = $1")
        .bind(conversation_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(title, "how many items?");

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn agent_requires_a_conversation_and_matching_mode() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;

    // Missing conversation_id → 400 (checked before any LLM config use).
    let (status, _) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"task": "x", "prompt": "hi"})),
    )
    .await
    .expect("missing conversation");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let classic = create_conversation(&st, &scratch.slug, scratch.user_id, "classic").await;
    let (status, _) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"task": "x", "prompt": "hi", "conversation_id": classic})),
    )
    .await
    .expect("mode mismatch");
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let other = scratch.add_actor(&pool, 20).await;
    let agent = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;
    let (status, _) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(other),
        Path(scratch.slug.clone()),
        Json(json!({"task": "x", "prompt": "hi", "conversation_id": agent})),
    )
    .await
    .expect("foreign conversation");
    assert_eq!(status, StatusCode::NOT_FOUND);

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn agent_failure_stores_an_error_message() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    // Point at a dead upstream so the agent fails.
    set_llm_env("http://127.0.0.1:1/v1");
    let (status, _) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({"task": "x", "prompt": "hi", "conversation_id": conversation_id})),
    )
    .await
    .expect("agent failure");
    clear_llm_env();
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

    let stored: Vec<(String, Value)> = sqlx::query_as(
        "SELECT role, metadata FROM ai_messages WHERE conversation_id = $1 ORDER BY created_at, id",
    )
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].0, "user");
    assert_eq!(stored[1].0, "assistant");
    assert_eq!(stored[1].1["is_error"], json!(true));

    scratch.purge(&pool).await;
}

#[tokio::test]
async fn agent_proposal_metadata_is_persisted_and_returned() {
    let pool = pool().await;
    let scratch = Scratch::new(&pool).await;
    let st = state(&pool).await;
    let conversation_id = create_conversation(&st, &scratch.slug, scratch.user_id, "agent").await;

    // Reuse the existing tool-call fake from `mod tool_roundtrip` (add the
    // `schedule_roundtrip_url()` helper there — see below).
    let base_url = tool_roundtrip::schedule_roundtrip_url().await;
    set_llm_env(&base_url);
    let (status, Json(body)) = api::routes::ai_agent::workspace_ai_agent(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path(scratch.slug.clone()),
        Json(json!({
            "task": "be helpful",
            "prompt": "/schedule daily report",
            "context": "ctx",
            "conversation_id": conversation_id,
        })),
    )
    .await
    .expect("agent call");
    clear_llm_env();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["pending_action"]["kind"], json!("create_schedule"));

    let metadata = &body["assistant_message"]["metadata"];
    assert_eq!(metadata["is_error"], json!(false));
    assert_eq!(metadata["schedule_decision"], json!("pending"));
    assert_eq!(metadata["schedule_proposal"]["frequency"], json!("daily"));
    assert!(metadata["schedule_proposal_key"].as_str().is_some());

    let stored: Value = sqlx::query_scalar(
        "SELECT metadata FROM ai_messages WHERE conversation_id = $1 AND role = 'assistant'",
    )
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored["schedule_decision"], json!("pending"));
    assert_eq!(stored["schedule_proposal"]["name"], json!("Daily"));

    scratch.purge(&pool).await;
}
