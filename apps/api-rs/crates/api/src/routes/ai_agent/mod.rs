//! `POST /api/workspaces/:slug/ai-agent/` — Rig tool-calling agent prototype.
//!
//! Demo-only surface: no Django counterpart and not consumed by the web app.
//! Runs an OpenAI-compatible chat-completions agent (Rig
//! `openai::CompletionsClient`) over typed, workspace-scoped read-only tools.
//! The `/ai-assistant/` parity contract is untouched.
//!
//! Rig 0.42 uses reqwest 0.13, the repo uses reqwest 0.12 — always pass
//! `rig::http_client::ReqwestClient`, never `reqwest::Client`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rig::completion::PromptError;
use rig::prelude::*;
use rig::providers::openai;
use rig::tool::server::ToolServerHandle;
use serde_json::Value;

use crate::routes::ai::LlmError;

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
