# Rust Rig Tool-Calling Agent Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Serve `POST /api/workspaces/:slug/ai-agent/` from Rust: an OpenAI-compatible Rig agent with three typed, workspace-scoped read-only tools over Postgres, demoing Rig's tool-calling loop without touching the existing `/ai-assistant/` parity contract.

**Architecture:** New `routes/ai_agent/` module (`mod.rs` handler + runner, `tools.rs` typed tools). The handler reuses `AuthUser`, `ws_role`/`guard_am`, and `resolve_llm_config()`. Rig runs a `CompletionsClient` agent over a `ToolServer` built from three `Tool` impls that hold `PgPool` + `workspace_id` (never model-supplied) and push a call trace from inside `Tool::call`. Integration tests use a stateful fake OpenAI-compatible upstream and a fake tool; no DB in tests.

**Tech Stack:** Rust 1.96 (axum 0.7, sqlx 0.7 Postgres, tokio), `rig = "=0.42.0"` (features `agent`, `reqwest`, `rustls`), `schemars = "1"`, existing parity gates (`route_inventory_test`, `fe_tripwire_test`).

**Spec:** `docs/superpowers/specs/2026-09-23-rust-rig-tool-agent-design.md`

**Spec corrections (verified by a real compile/run spike against rig 0.42):**

1. `run_agent` does **not** take a trace parameter: each tool owns a clone of a `ToolTrace` handle and records its own call. The runner signature is `(base_url, api_key, model, tool_server, prompt)`.
2. Rig 0.42 depends on **reqwest 0.13**, the repo uses reqwest 0.12. Do **not** pass `reqwest::Client` (0.12) to Rig — `HttpClientExt` is implemented for 0.13. Use the re-exported `rig::http_client::ReqwestClient` (0.13) for the 60s timeout. Both versions coexist; no API breakage for existing code.
3. Turn budget is `AgentBuilder::default_max_turns(6)` (not `multi_turn`); Rig's default is 1, which would make tool loops impossible.
4. Use `openai::CompletionsClient` (chat completions), never `openai::Client` (Responses API, unsupported by OpenRouter).
5. Verified upstream error inspection: `PromptError::provider_response_status() == Some(429)`.

---

## File map

| Action | File                                                  | Responsibility                                                           |
| ------ | ----------------------------------------------------- | ------------------------------------------------------------------------ |
| Create | `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`   | Handler, preamble, `run_agent`, trace type, error mapping, unit tests    |
| Create | `apps/api-rs/crates/api/src/routes/ai_agent/tools.rs` | 3 typed tools, static SQL, validation/output helpers, unit tests         |
| Create | `apps/api-rs/crates/api/tests/ai_agent_test.rs`       | Stateful fake-upstream integration tests (tool round-trip, 429/500/JSON) |
| Modify | `apps/api-rs/Cargo.toml`                              | Workspace deps `rig`, `schemars`                                         |
| Modify | `apps/api-rs/crates/api/Cargo.toml`                   | Use workspace `rig`, `schemars`                                          |
| Modify | `apps/api-rs/crates/api/src/routes/mod.rs`            | Register `pub mod ai_agent;`                                             |
| Modify | `apps/api-rs/crates/api/src/main.rs`                  | Route `POST /api/workspaces/:slug/ai-agent/`                             |
| Modify | `apps/api-rs/crates/api/src/routes/ai.rs`             | `host_of` → `pub(crate)` (visibility only)                               |
| Modify | `apps/api-rs/Cargo.lock`                              | Lockfile from `cargo check`                                              |

No changes to `parity-inventory.json` (no Django counterpart, not consumed by FE), no FE changes, no migrations.

---

### Task 1: Dependencies + module skeleton + no-tool round-trip

**Files:**

- Modify: `apps/api-rs/Cargo.toml`
- Modify: `apps/api-rs/crates/api/Cargo.toml`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`
- Create: `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`
- Test: `apps/api-rs/crates/api/tests/ai_agent_test.rs`

- [ ] **Step 1: Add dependencies**

In `apps/api-rs/Cargo.toml` under `[workspace.dependencies]` (after `regex = "1"`):

```toml
rig = { version = "=0.42.0", default-features = false, features = ["agent", "reqwest", "rustls"] }
schemars = "1"
```

In `apps/api-rs/crates/api/Cargo.toml` under `[dependencies]` (after `regex = { workspace = true }`):

```toml
rig = { workspace = true }
schemars = { workspace = true }
```

Pin `=0.42.0` exactly: Rig is 0.x and ships breaking changes between minors.

- [ ] **Step 2: Register the module**

In `apps/api-rs/crates/api/src/routes/mod.rs`, after `pub mod ai;` add:

```rust
pub mod ai_agent;
```

- [ ] **Step 3: Write the failing integration test**

Create `apps/api-rs/crates/api/tests/ai_agent_test.rs`:

```rust
//! Fake OpenAI-compatible upstream for `routes::ai_agent::run_agent`:
//! no-tool answer, tool round-trip, 429, 500, malformed JSON. No DB, no env.

use axum::{http::StatusCode, routing::post, Json, Router};
use api::routes::ai_agent::run_agent;
use rig::tool::server::ToolServer;
use serde_json::{json, Value};

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
        axum::serve(listener, Router::new().route("/v1/chat/completions", post(handler)))
            .await
            .unwrap();
    });
    format!("http://{addr}/v1")
}

#[tokio::test]
async fn prompt_without_tools_returns_content() {
    let base = spawn_fixed(200, chat_response("final answer")).await;
    let out = run_agent(&base, "key", "gpt-4o-mini", ToolServer::new().run(), "hi").await;
    assert_eq!(out, Ok("final answer".to_string()));
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cargo test -p api --test ai_agent_test 2>&1 | tail -20`
Expected: compile error — unresolved import `api::routes::ai_agent`.

- [ ] **Step 5: Write the minimal runner**

Create `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`:

```rust
//! `POST /api/workspaces/:slug/ai-agent/` — Rig tool-calling agent prototype.
//!
//! Demo-only surface: no Django counterpart and not consumed by the web app.
//! Runs an OpenAI-compatible chat-completions agent (Rig
//! `openai::CompletionsClient`) over typed, workspace-scoped read-only tools.
//! The `/ai-assistant/` parity contract is untouched.
//!
//! Rig 0.42 uses reqwest 0.13, the repo uses reqwest 0.12 — always pass
//! `rig::http_client::ReqwestClient`, never `reqwest::Client`.

use std::time::Duration;

use rig::completion::PromptError;
use rig::prelude::*;
use rig::providers::openai;
use rig::tool::server::ToolServerHandle;

use crate::routes::ai::LlmError;

// Handler-only imports (axum, Value, Uuid, resolve_llm_config, guard_am,
// deny, ws_role, AuthUser, AppState) are added in Task 5 when the handler
// lands — keeping this task warning-free.

pub const PREAMBLE: &str = "You are the workspace AI assistant for Plane. \
Answer factual questions about projects and work items by calling the provided \
tools; never invent project identifiers, work item identifiers, counts, or \
states. All tools are read-only and scoped to the user's current workspace. If \
a tool returns no results, say so. Answer concisely in the user's language.";

/// Total model-call budget: initial call + every tool round-trip continuation.
pub const MAX_TURNS: usize = 6;

/// Django-style lax parsing like `routes/ai.rs::task_from_body`.
pub fn prompt_from_body(body: &Value) -> Option<&str> {
    body.get("prompt").and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn map_prompt_error(error: PromptError) -> LlmError {
    if error.provider_response_status().map(|s| s.as_u16()) == Some(429) {
        return LlmError::RateLimited;
    }
    tracing::warn!(error = %error, "ai-agent: upstream failed");
    LlmError::Upstream
}

/// Build the Rig client and run one agent prompt. Split from the handler so
/// integration tests can drive it against a fake upstream without a DB.
pub async fn run_agent(
    base_url: &str,
    api_key: &str,
    model: &str,
    tool_server: ToolServerHandle,
    prompt: &str,
) -> Result<String, LlmError> {
    let http = rig::http_client::ReqwestClient::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| {
            tracing::warn!(error = %e, "ai-agent: http client build failed");
            LlmError::Upstream
        })?;
    let client = openai::CompletionsClient::builder()
        .api_key(api_key)
        .base_url(base_url)
        .http_client(http)
        .build()
        .map_err(|e| {
            tracing::warn!(error = %e, "ai-agent: llm client build failed");
            LlmError::Upstream
        })?;
    let agent = client
        .agent(model)
        .preamble(PREAMBLE)
        .tool_server_handle(tool_server)
        .default_max_turns(MAX_TURNS)
        .build();
    agent.prompt(prompt).await.map_err(map_prompt_error)
}
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test -p api --test ai_agent_test 2>&1 | tail -20`
Expected: `test prompt_without_tools_returns_content ... ok` (first build downloads/compiles Rig; allow several minutes).

- [ ] **Step 7: Commit**

```bash
git add apps/api-rs/Cargo.toml apps/api-rs/Cargo.lock apps/api-rs/crates/api/Cargo.toml \
  apps/api-rs/crates/api/src/routes/mod.rs apps/api-rs/crates/api/src/routes/ai_agent/mod.rs \
  apps/api-rs/crates/api/tests/ai_agent_test.rs
git commit -m "feat(api-rs): rig dependency and no-tool ai-agent runner"
```

---

### Task 2: Tool round-trip, trace, and error mapping

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`
- Modify: `apps/api-rs/crates/api/tests/ai_agent_test.rs`

- [ ] **Step 1: Write the failing tests**

Append to `apps/api-rs/crates/api/tests/ai_agent_test.rs` (and remove the now-unused outer `use api::routes::ai::LlmError;` line if present — the nested module imports `LlmError` itself):

```rust
mod tool_roundtrip {
    use std::sync::{Arc, Mutex};

    use api::routes::ai::LlmError;
    use api::routes::ai_agent::{new_trace, run_agent, ToolCallTrace, ToolTrace};
    use axum::{
        extract::State,
        http::StatusCode,
        routing::post,
        Json, Router,
    };
    use rig::tool::server::ToolServer;
    use rig::tool::{Tool, ToolContext, ToolExecutionError};
    use serde_json::{json, Value};

    #[derive(serde::Deserialize, schemars::JsonSchema)]
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

    async fn handler(State(state): State<Shared>, Json(body): Json<Value>) -> (StatusCode, Json<Value>) {
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

    #[tokio::test]
    async fn tool_call_roundtrip_records_trace_and_returns_final_text() {
        let (base, upstream) = spawn_roundtrip().await;
        let trace = new_trace();
        let tool = FakeEcho { trace: trace.clone() };
        let out = run_agent(&base, "key", "model", ToolServer::new().tool(tool).run(), "say hi")
            .await;
        assert_eq!(out, Ok("final answer".to_string()));

        let recorded = trace.lock().unwrap().clone();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].name, "echo");
        assert_eq!(recorded[0].arguments["text"], json!("hi"));

        let bodies = upstream.bodies.lock().unwrap();
        assert_eq!(bodies.len(), 2, "tool loop must issue a second upstream call");
        let messages = bodies[1]["messages"].as_array().unwrap();
        let tool_msg = messages
            .iter()
            .find(|m| m["role"] == json!("tool"))
            .expect("second call must carry the tool result message");
        assert_eq!(tool_msg["content"], json!("echo:hi"));
    }

    #[tokio::test]
    async fn upstream_429_maps_to_rate_limited() {
        let base = super::spawn_fixed(429, json!({"error": {"message": "slow down"}})).await;
        let out = run_agent(&base, "key", "model", ToolServer::new().run(), "hi").await;
        assert_eq!(out, Err(LlmError::RateLimited));
    }

    #[tokio::test]
    async fn upstream_500_maps_to_upstream() {
        let base = super::spawn_fixed(500, json!({"error": "boom"})).await;
        let out = run_agent(&base, "key", "model", ToolServer::new().run(), "hi").await;
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
        let out = run_agent(&base, "key", "model", ToolServer::new().run(), "hi").await;
        assert_eq!(out, Err(LlmError::Upstream));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p api --test ai_agent_test 2>&1 | tail -20`
Expected: compile error — `new_trace`, `ToolCallTrace`, `ToolTrace` not found.

- [ ] **Step 3: Add trace types**

In `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`, add after the imports:

```rust
use std::sync::{Arc, Mutex};

/// One recorded tool invocation, surfaced in the 200 response as `tool_calls`.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallTrace {
    pub name: String,
    pub arguments: Value,
}

/// Shared trace handle; each tool owns a clone and records from `Tool::call`.
pub type ToolTrace = Arc<Mutex<Vec<ToolCallTrace>>>;

pub fn new_trace() -> ToolTrace {
    Arc::new(Mutex::new(Vec::new()))
}

/// Best-effort trace push: a poisoned lock or unserializable args never break
/// a tool call.
pub fn record(trace: &ToolTrace, name: &str, arguments: &impl serde::Serialize) {
    let Ok(arguments) = serde_json::to_value(arguments) else {
        return;
    };
    if let Ok(mut recorded) = trace.lock() {
        recorded.push(ToolCallTrace {
            name: name.to_string(),
            arguments,
        });
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p api --test ai_agent_test 2>&1 | tail -20`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_agent/mod.rs apps/api-rs/crates/api/tests/ai_agent_test.rs
git commit -m "feat(api-rs): tool round-trip trace and error mapping for ai-agent"
```

---

### Task 3: Tool SQL, validation, and output helpers

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/ai_agent/tools.rs`
- Modify: `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`

- [ ] **Step 1: Write the failing unit tests**

Create `apps/api-rs/crates/api/src/routes/ai_agent/tools.rs` containing ONLY this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sql_constants_are_workspace_scoped() {
        for sql in [PROJECTS_SQL, COUNT_SQL, SEARCH_SQL] {
            assert!(sql.contains("workspace_id = $1"), "missing workspace scope: {sql}");
        }
    }

    #[test]
    fn state_group_allowlist() {
        assert_eq!(state_group_arg(None).unwrap(), None);
        assert_eq!(state_group_arg(Some("  ")).unwrap(), None);
        assert_eq!(state_group_arg(Some("Started")).unwrap(), Some("started".to_string()));
        let err = state_group_arg(Some("nope")).unwrap_err();
        assert!(err.to_string().contains("backlog"));
    }

    #[test]
    fn priority_allowlist() {
        assert_eq!(priority_arg(None).unwrap(), None);
        assert_eq!(priority_arg(Some("URGENT")).unwrap(), Some("urgent".to_string()));
        let err = priority_arg(Some("p0")).unwrap_err();
        assert!(err.to_string().contains("urgent"));
    }

    #[test]
    fn limit_is_clamped() {
        assert_eq!(clamp_limit(None), 10);
        assert_eq!(clamp_limit(Some(0)), 1);
        assert_eq!(clamp_limit(Some(3)), 3);
        assert_eq!(clamp_limit(Some(999)), 25);
    }

    #[test]
    fn optional_text_trims_and_drops_empty() {
        assert_eq!(optional_text(None), None);
        assert_eq!(optional_text(Some("  ")), None);
        assert_eq!(optional_text(Some(" LT ")), Some("LT".to_string()));
    }

    #[test]
    fn output_json_shapes() {
        let projects = projects_json(&[("LTS".to_string(), "Logistics".to_string())]);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&projects).unwrap(),
            json!({"items": [{"identifier": "LTS", "name": "Logistics"}]})
        );
        let count = count_json(7, Some("LTS"), Some("started"), Some("urgent"), false);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&count).unwrap(),
            json!({
                "count": 7,
                "filters": {
                    "project": "LTS",
                    "state_group": "started",
                    "priority": "urgent",
                    "include_archived": false
                }
            })
        );
        let search = search_json(&[(
            "LTS-12".to_string(),
            "Fix pump".to_string(),
            "In Progress".to_string(),
            "urgent".to_string(),
        )]);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&search).unwrap(),
            json!({"count": 1, "items": [{
                "identifier": "LTS-12",
                "name": "Fix pump",
                "state": "In Progress",
                "priority": "urgent"
            }]})
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p api --lib routes::ai_agent::tools 2>&1 | tail -20`
Expected: compile error — `PROJECTS_SQL`, `state_group_arg`, etc. not found.

- [ ] **Step 3: Implement the helpers**

At the top of `apps/api-rs/crates/api/src/routes/ai_agent/tools.rs` (above the `#[cfg(test)]` module):

```rust
//! Read-only, workspace-scoped tools for the Rig agent.
//!
//! Every query filters `workspace_id = $1` captured from the authenticated
//! handler; the model never chooses the workspace. Filters are optional
//! nullable bind parameters (`$n::text IS NULL OR ...`) so the SQL stays
//! static and the pure helpers below are unit-testable without a DB.

use rig::tool::{Tool, ToolContext, ToolExecutionError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{record, ToolTrace};

pub const PROJECTS_SQL: &str = "SELECT identifier, name FROM projects \
     WHERE workspace_id = $1 AND deleted_at IS NULL AND archived_at IS NULL \
     ORDER BY name LIMIT 50";

pub const COUNT_SQL: &str = "SELECT count(*)::int8 FROM issues i \
     JOIN projects p ON p.id = i.project_id \
     LEFT JOIN states s ON s.id = i.state_id \
     WHERE i.workspace_id = $1 AND i.deleted_at IS NULL \
     AND ($2::text IS NULL OR p.identifier ILIKE $2 OR p.name ILIKE '%' || $2 || '%') \
     AND ($3::text IS NULL OR s.\"group\" = $3) \
     AND ($4::text IS NULL OR i.priority = $4) \
     AND ($5::bool OR i.archived_at IS NULL)";

pub const SEARCH_SQL: &str = "SELECT p.identifier || '-' || i.sequence_id AS identifier, \
     i.name, COALESCE(s.name, '') AS state, i.priority \
     FROM issues i \
     JOIN projects p ON p.id = i.project_id \
     LEFT JOIN states s ON s.id = i.state_id \
     WHERE i.workspace_id = $1 AND i.deleted_at IS NULL AND i.archived_at IS NULL \
     AND ($2::text IS NULL OR i.name ILIKE '%' || $2 || '%') \
     AND ($3::text IS NULL OR p.identifier ILIKE $3 OR p.name ILIKE '%' || $3 || '%') \
     AND ($4::text IS NULL OR s.\"group\" = $4) \
     AND ($5::text IS NULL OR i.priority = $5) \
     ORDER BY i.updated_at DESC LIMIT $6";

pub const STATE_GROUPS: [&str; 5] = ["backlog", "unstarted", "started", "completed", "cancelled"];
pub const PRIORITIES: [&str; 5] = ["urgent", "high", "medium", "low", "none"];
pub const DEFAULT_LIMIT: i64 = 10;
pub const MAX_LIMIT: i64 = 25;

pub fn optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn state_group_arg(value: Option<&str>) -> Result<Option<String>, ToolExecutionError> {
    let Some(normalized) = optional_text(value).map(|s| s.to_ascii_lowercase()) else {
        return Ok(None);
    };
    if STATE_GROUPS.contains(&normalized.as_str()) {
        Ok(Some(normalized))
    } else {
        Err(ToolExecutionError::invalid_args(format!(
            "state_group must be one of: {}",
            STATE_GROUPS.join(", ")
        )))
    }
}

pub fn priority_arg(value: Option<&str>) -> Result<Option<String>, ToolExecutionError> {
    let Some(normalized) = optional_text(value).map(|s| s.to_ascii_lowercase()) else {
        return Ok(None);
    };
    if PRIORITIES.contains(&normalized.as_str()) {
        Ok(Some(normalized))
    } else {
        Err(ToolExecutionError::invalid_args(format!(
            "priority must be one of: {}",
            PRIORITIES.join(", ")
        )))
    }
}

pub fn clamp_limit(value: Option<i64>) -> i64 {
    value.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

pub fn projects_json(rows: &[(String, String)]) -> String {
    let items: Vec<Value> = rows
        .iter()
        .map(|(identifier, name)| json!({"identifier": identifier, "name": name}))
        .collect();
    json!({"items": items}).to_string()
}

pub fn count_json(
    count: i64,
    project: Option<&str>,
    state_group: Option<&str>,
    priority: Option<&str>,
    include_archived: bool,
) -> String {
    json!({
        "count": count,
        "filters": {
            "project": project,
            "state_group": state_group,
            "priority": priority,
            "include_archived": include_archived
        }
    })
    .to_string()
}

pub fn search_json(rows: &[(String, String, String, String)]) -> String {
    let items: Vec<Value> = rows
        .iter()
        .map(|(identifier, name, state, priority)| {
            json!({"identifier": identifier, "name": name, "state": state, "priority": priority})
        })
        .collect();
    json!({"count": items.len(), "items": items}).to_string()
}
```

Note: `Tool`, `ToolContext`, `ToolExecutionError`, `record`, and `ToolTrace` are consumed by Task 4; `cargo test` may show them as unused imports after this task alone — that is expected and resolves in Task 4 (do not remove them).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p api --lib routes::ai_agent::tools 2>&1 | tail -20`
Expected: 6 passed.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_agent/tools.rs apps/api-rs/crates/api/src/routes/ai_agent/mod.rs
git commit -m "feat(api-rs): workspace tool sql helpers for ai-agent"
```

Add `pub mod tools;` to `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs` when creating the file (Task 5's handler also references `tools::workspace_tools`).

---

### Task 4: Typed tools

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai_agent/tools.rs`

- [ ] **Step 1: Write the failing tests**

Append inside the existing `#[cfg(test)] mod tests` in `tools.rs`:

```rust
    fn lazy_pool() -> PgPool {
        sqlx::PgPool::connect_lazy("postgres://user:pass@127.0.0.1:1/plane").expect("lazy pool")
    }

    #[test]
    fn tool_metadata_is_exposed() {
        let pool = lazy_pool();
        let trace = super::super::new_trace();

        let list = ListProjects { pool: pool.clone(), workspace_id: Uuid::nil(), trace: trace.clone() };
        assert_eq!(ListProjects::NAME, "list_projects");
        assert!(!list.description().is_empty());
        assert_eq!(list.parameters()["type"], json!("object"));

        let count = CountWorkItems { pool: pool.clone(), workspace_id: Uuid::nil(), trace: trace.clone() };
        let count_params = count.parameters();
        assert!(count_params["properties"]["project"].is_object());
        assert!(count_params["properties"]["state_group"].is_object());
        assert!(count_params["properties"]["priority"].is_object());
        assert!(count_params["properties"]["include_archived"].is_object());

        let search = SearchWorkItems { pool, workspace_id: Uuid::nil(), trace };
        let search_params = search.parameters();
        assert!(search_params["properties"]["query"].is_object());
        assert!(search_params["properties"]["limit"].is_object());
        assert!(search_params["properties"]["project"].is_object());
        assert!(!search.description().is_empty());
    }

    #[test]
    fn workspace_tools_builds_a_server_handle() {
        let _handle = workspace_tools(lazy_pool(), Uuid::nil(), super::super::new_trace());
    }
```

Add `use uuid::Uuid;` inside the test module if not already in scope via `use super::*;` (it is, because the parent imports `Uuid`).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p api --lib routes::ai_agent::tools 2>&1 | tail -20`
Expected: compile error — `ListProjects`, `CountWorkItems`, `SearchWorkItems`, `workspace_tools` not found.

- [ ] **Step 3: Implement the tools**

Append to `apps/api-rs/crates/api/src/routes/ai_agent/tools.rs` (above the test module):

```rust
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct ListProjectsArgs {}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CountWorkItemsArgs {
    /// Project identifier (e.g. "LTS") or project name, case-insensitive substring.
    pub project: Option<String>,
    /// One of: backlog, unstarted, started, completed, cancelled.
    pub state_group: Option<String>,
    /// One of: urgent, high, medium, low, none.
    pub priority: Option<String>,
    /// Include archived work items. Defaults to false.
    pub include_archived: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SearchWorkItemsArgs {
    /// Case-insensitive substring to match against work item names.
    pub query: Option<String>,
    /// Project identifier (e.g. "LTS") or project name, case-insensitive substring.
    pub project: Option<String>,
    /// One of: backlog, unstarted, started, completed, cancelled.
    pub state_group: Option<String>,
    /// One of: urgent, high, medium, low, none.
    pub priority: Option<String>,
    /// Maximum rows to return, 1-25 (default 10).
    pub limit: Option<i64>,
}

fn db_error(error: sqlx::Error) -> ToolExecutionError {
    tracing::warn!(error = %error, "ai-agent: tool query failed");
    ToolExecutionError::from_error(error)
}

fn schema_of<T: JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T))
        .unwrap_or_else(|_| json!({"type": "object", "properties": {}}))
}

pub struct ListProjects {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for ListProjects {
    const NAME: &'static str = "list_projects";
    type Args = ListProjectsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "List non-archived projects in the current workspace with identifier and name.".to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<ListProjectsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let rows: Vec<(String, String)> = sqlx::query_as(PROJECTS_SQL)
            .bind(self.workspace_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(projects_json(&rows))
    }
}

pub struct CountWorkItems {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for CountWorkItems {
    const NAME: &'static str = "count_work_items";
    type Args = CountWorkItemsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Count non-deleted work items in the current workspace, optionally filtered by project, state group, priority, and archived status.".to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<CountWorkItemsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let project = optional_text(args.project.as_deref());
        let state_group = state_group_arg(args.state_group.as_deref())?;
        let priority = priority_arg(args.priority.as_deref())?;
        let include_archived = args.include_archived.unwrap_or(false);
        let count: i64 = sqlx::query_scalar(COUNT_SQL)
            .bind(self.workspace_id)
            .bind(&project)
            .bind(&state_group)
            .bind(&priority)
            .bind(include_archived)
            .fetch_one(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(count_json(
            count,
            project.as_deref(),
            state_group.as_deref(),
            priority.as_deref(),
            include_archived,
        ))
    }
}

pub struct SearchWorkItems {
    pub pool: PgPool,
    pub workspace_id: Uuid,
    pub trace: ToolTrace,
}

impl Tool for SearchWorkItems {
    const NAME: &'static str = "search_work_items";
    type Args = SearchWorkItemsArgs;
    type Output = String;
    type Error = ToolExecutionError;

    fn description(&self) -> String {
        "Search non-archived work items in the current workspace by name substring, optionally filtered by project, state group, and priority. Returns identifier, name, state, and priority.".to_string()
    }

    fn parameters(&self) -> Value {
        schema_of::<SearchWorkItemsArgs>()
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        record(&self.trace, Self::NAME, &args);
        let query = optional_text(args.query.as_deref());
        let project = optional_text(args.project.as_deref());
        let state_group = state_group_arg(args.state_group.as_deref())?;
        let priority = priority_arg(args.priority.as_deref())?;
        let limit = clamp_limit(args.limit);
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(SEARCH_SQL)
            .bind(self.workspace_id)
            .bind(&query)
            .bind(&project)
            .bind(&state_group)
            .bind(&priority)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(search_json(&rows))
    }
}

/// Build the production tool server: three read-only tools scoped to one
/// workspace, all sharing the caller's trace handle.
pub fn workspace_tools(
    pool: PgPool,
    workspace_id: Uuid,
    trace: ToolTrace,
) -> rig::tool::server::ToolServerHandle {
    rig::tool::server::ToolServer::new()
        .tool(ListProjects {
            pool: pool.clone(),
            workspace_id,
            trace: trace.clone(),
        })
        .tool(CountWorkItems {
            pool: pool.clone(),
            workspace_id,
            trace: trace.clone(),
        })
        .tool(SearchWorkItems {
            pool,
            workspace_id,
            trace,
        })
        .run()
}
```

Also remove the now-unneeded `use std::sync::{Arc, Mutex};` and `use super::{record, ToolCallTrace, ToolTrace};` if the compiler warns about unused imports (`Arc`/`Mutex`/`ToolCallTrace` are not used in this file after implementation; `record`, `ToolTrace`, `Tool`, `ToolContext`, `ToolExecutionError` are).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p api --lib routes::ai_agent 2>&1 | tail -20`
Expected: 8 passed, no warnings.

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai_agent/tools.rs
git commit -m "feat(api-rs): typed read-only workspace tools for ai-agent"
```

---

### Task 5: Handler, route, and `host_of` visibility

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai.rs` (visibility only)
- Modify: `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs`

- [ ] **Step 1: Lock the parsing helpers with regression tests**

Append to `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn prompt_from_body_rules() {
        assert_eq!(prompt_from_body(&json!({"prompt": "hi"})), Some("hi"));
        assert_eq!(prompt_from_body(&json!({})), None);
        assert_eq!(prompt_from_body(&json!({"prompt": ""})), None);
        assert_eq!(prompt_from_body(&json!({"prompt": 5})), None);
        assert_eq!(prompt_from_body(&json!({"prompt": null})), None);
    }

    #[test]
    fn record_appends_and_serializes() {
        let trace = new_trace();
        record(&trace, "search_work_items", &json!({"query": "pump"}));
        let recorded = trace.lock().unwrap().clone();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].name, "search_work_items");
        assert_eq!(recorded[0].arguments["query"], json!("pump"));
    }
}
```

- [ ] **Step 2: Run the tests to verify they pass**

Run: `cargo test -p api --lib routes::ai_agent 2>&1 | tail -20`
Expected: 10 passed (`prompt_from_body`/`record` landed in Tasks 1–2, so these are regression tests; the handler itself is compile-verified in Step 6 and exercised by the live smoke in Task 6).

- [ ] **Step 3: Make `host_of` reusable**

In `apps/api-rs/crates/api/src/routes/ai.rs`, change:

```rust
fn host_of(base_url: &str) -> String {
```

to:

```rust
pub(crate) fn host_of(base_url: &str) -> String {
```

No behavior change; `/ai-assistant/` and its tests stay byte-identical.

- [ ] **Step 4: Add the handler**

Append to `apps/api-rs/crates/api/src/routes/ai_agent/mod.rs` (above the test module; `pub mod tools;` already exists from Task 3):

```rust
/// `POST /api/workspaces/:slug/ai-agent/`.
///
/// Gate: workspace ADMIN/MEMBER. Errors mirror `/ai-assistant/` messages so
/// operators see a consistent surface.
pub async fn workspace_ai_agent(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let workspace_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL")
            .bind(&slug)
            .fetch_optional(&st.pool)
            .await?;
    let Some(workspace_id) = workspace_id else {
        return Ok(deny());
    };
    let cfg = resolve_llm_config(&st.pool).await;
    if cfg.api_key.is_empty() || cfg.model.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "AI is not configured for this workspace."})),
        ));
    }
    let Some(prompt) = prompt_from_body(&body) else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Prompt is required"})),
        ));
    };
    let trace = new_trace();
    let tool_server = tools::workspace_tools(st.pool.clone(), workspace_id, trace.clone());
    match run_agent(&cfg.base_url, &cfg.api_key, &cfg.model, tool_server, prompt).await {
        Ok(text) => {
            let tool_calls: Vec<Value> = trace
                .lock()
                .map(|recorded| {
                    recorded
                        .iter()
                        .map(|call| json!({"name": call.name, "arguments": call.arguments}))
                        .collect()
                })
                .unwrap_or_default();
            Ok((
                StatusCode::OK,
                Json(json!({"response": text, "tool_calls": tool_calls})),
            ))
        }
        Err(LlmError::RateLimited) => Ok((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": format!("Rate limit exceeded for {}", host_of(&cfg.base_url))})),
        )),
        Err(LlmError::Upstream) => Ok((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "An internal error has occurred."})),
        )),
    }
}
```

Replace the import block in `mod.rs` with the full set needed by the handler (Task 1 deliberately kept it minimal):

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::routes::ai::{host_of, resolve_llm_config, LlmError};
use crate::routes::module::guard_am;
use crate::routes::project::{deny, ws_role};
use crate::{middleware::auth::AuthUser, state::AppState};
```

(The `std::time::Duration`, `rig::completion::PromptError`, `rig::prelude::*`, `rig::providers::openai`, and `rig::tool::server::ToolServerHandle` imports from Task 1 stay.)

- [ ] **Step 5: Register the route**

In `apps/api-rs/crates/api/src/main.rs`, after the `/api/workspaces/:slug/ai-assistant/` route (around line 1758), add:

```rust
        .route(
            "/api/workspaces/:slug/ai-agent/",
            post(routes::ai_agent::workspace_ai_agent),
        )
```

- [ ] **Step 6: Run the full test suite and build**

Run: `cargo test -p api 2>&1 | tail -30`
Expected: all tests pass, including `route_inventory_test` and `fe_tripwire_test` (route-only gate is one-directional; no inventory edit needed).

Run: `cargo check -p api 2>&1 | tail -10`
Expected: `Finished` with no warnings.

- [ ] **Step 7: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai.rs \
  apps/api-rs/crates/api/src/routes/ai_agent/mod.rs \
  apps/api-rs/crates/api/src/main.rs
git commit -m "feat(api-rs): ai-agent route and workspace-scoped handler"
```

---

### Task 6: Formatting, lint, live smoke

**Files:** none new (format/lint fixes only, if any).

- [ ] **Step 1: Format and lint**

Run:

```bash
cargo fmt --all
cargo clippy -p api --all-targets 2>&1 | tail -20
```

Expected: no clippy errors. Fix any lint findings in `ai_agent/mod.rs` / `ai_agent/tools.rs` / `ai_test`-style tests, then re-run.

- [ ] **Step 2: Full suite**

Run: `cargo test -p api 2>&1 | tail -20`
Expected: all tests pass.

- [ ] **Step 3: Live smoke (manual, needs a configured deployment)**

Preconditions: `LLM_API_KEY` + `LLM_MODEL` set via the admin AI form or env, `LLM_BASE_URL` env set if not OpenAI (this deployment: `https://openrouter.ai/api/v1`), and the model supports tool calling.

```bash
# rebuild + restart (from repo root)
docker compose build api && docker compose up -d api

# use a workspace slug where the token user is ADMIN or MEMBER
SLUG=<workspace-slug>
TOKEN=<session token>

# happy path: expect 200 with a non-empty tool_calls array and a data-backed answer
curl -sS -X POST "http://localhost:8000/api/workspaces/$SLUG/ai-agent/" \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"prompt":"Berapa banyak work item urgent di workspace ini?"}' | jq

# negatives
curl -sS -o /dev/null -w '%{http_code}\n' -X POST \
  "http://localhost:8000/api/workspaces/$SLUG/ai-agent/" \
  -H 'Content-Type: application/json' -d '{"prompt":"x"}'          # expect 401
curl -sS -X POST "http://localhost:8000/api/workspaces/$SLUG/ai-agent/" \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{}' | jq                                                     # expect 400 Prompt is required
```

If the model answers without calling tools, check that `LLM_MODEL` supports function calling and re-read `tool_calls` in the response. If the provider rejects `list_projects` (empty `properties` schema), fall back to adding an optional `query: Option<String>` arg to `ListProjectsArgs` and its SQL (documented fallback; do not change other tools).

- [ ] **Step 4: Commit any fixes**

```bash
git add -A apps/api-rs
git commit -m "chore(api-rs): ai-agent formatting and lint fixes"
```

(Rollback for the whole feature: revert the five feature commits and rebuild; no DB state is created by this route.)
