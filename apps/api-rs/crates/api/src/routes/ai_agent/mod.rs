//! `POST /api/workspaces/:slug/ai-agent/` — Rig tool-calling agent prototype.
//!
//! Demo-only surface: no Django counterpart and not consumed by the web app.
//! Runs an OpenAI-compatible chat-completions agent (Rig
//! `openai::CompletionsClient`) over typed, workspace-scoped read-only tools.
//! The `/ai-assistant/` parity contract is untouched.
//!
//! Rig 0.42 uses reqwest 0.13, the repo uses reqwest 0.12 — always pass
//! `rig::http_client::ReqwestClient`, never `reqwest::Client`.

use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use rig::completion::PromptError;
use rig::prelude::*;
use rig::providers::openai;
use rig::tool::server::ToolServerHandle;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::routes::ai::{host_of, resolve_llm_config, task_from_body, LlmError};
use crate::routes::module::guard_am;
use crate::routes::project::{deny, ws_role};
use crate::{middleware::auth::AuthUser, state::AppState};

pub mod tools;

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

pub const PREAMBLE: &str = "You are the workspace AI assistant for Plane. \
Answer factual questions about projects and work items by calling the provided \
tools; never invent project identifiers, work item identifiers, counts, or \
states. All tools are read-only and scoped to the user's current workspace. If \
a tool returns no results, say so. Answer concisely in the user's language.";

/// Total model-call budget: initial call + every tool round-trip continuation.
pub const MAX_TURNS: usize = 6;

/// Total wall-clock budget for one request: all model calls plus tool time.
pub const AGENT_TIMEOUT: Duration = Duration::from_secs(180);

/// Django-style lax parsing like `routes/ai.rs::task_from_body`.
pub fn prompt_from_body(body: &Value) -> Option<&str> {
    body.get("prompt")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// Effective user message: Django parity concatenates `task + "\n" + prompt`
/// (`routes/ai.rs::build_body`); an absent task leaves the prompt as-is.
pub fn effective_prompt(task: Option<&str>, prompt: &str) -> String {
    match task {
        Some(task) => format!("{task}\n{prompt}"),
        None => prompt.to_string(),
    }
}

fn map_prompt_error(error: PromptError) -> LlmError {
    if error.provider_response_status().map(|s| s.as_u16()) == Some(429) {
        tracing::warn!("ai-agent: upstream rate limited");
        return LlmError::RateLimited;
    }
    tracing::warn!(error = %error, "ai-agent: upstream failed");
    LlmError::Upstream
}

/// Shared HTTP client for all ai-agent requests (keeps connection pooling).
fn http_client() -> &'static rig::http_client::ReqwestClient {
    static HTTP: OnceLock<rig::http_client::ReqwestClient> = OnceLock::new();
    HTTP.get_or_init(|| {
        rig::http_client::ReqwestClient::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("ai-agent http client")
    })
}

/// Build the Rig client and run one agent prompt. Split from the handler so
/// integration tests can drive it against a fake upstream without a DB.
pub async fn run_agent(
    base_url: &str,
    api_key: &str,
    model: &str,
    tool_server: ToolServerHandle,
    task: Option<&str>,
    prompt: &str,
) -> Result<String, LlmError> {
    let client = openai::CompletionsClient::builder()
        .api_key(api_key)
        .base_url(base_url)
        .http_client(http_client().clone())
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
    agent
        .prompt(effective_prompt(task, prompt))
        .await
        .map_err(map_prompt_error)
}

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
    let task = task_from_body(&body);
    let trace = new_trace();
    let tool_server = tools::workspace_tools(st.pool.clone(), workspace_id, trace.clone());
    let agent_result = tokio::time::timeout(
        AGENT_TIMEOUT,
        run_agent(
            &cfg.base_url,
            &cfg.api_key,
            &cfg.model,
            tool_server,
            task,
            prompt,
        ),
    )
    .await
    .unwrap_or_else(|_| {
        tracing::warn!("ai-agent: request timed out");
        Err(LlmError::Upstream)
    });
    match agent_result {
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
    fn effective_prompt_folds_task_like_django() {
        assert_eq!(effective_prompt(Some("do it"), "text"), "do it\ntext");
        assert_eq!(effective_prompt(None, "text"), "text");
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
