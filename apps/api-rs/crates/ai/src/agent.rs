//! Rig tool-calling agent runtime, shared by the HTTP handler and scheduled
//! runs.

use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rig::completion::PromptError;
use rig::prelude::*;
use rig::providers::openai;
use rig::tool::server::ToolServerHandle;
use serde_json::Value;

use crate::llm::LlmError;
use crate::tools::CREATE_SCHEDULE_NAME;

/// One recorded tool invocation, surfaced in the 200 response as `tool_calls`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
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
states. All tools are scoped to the user's current workspace and read-only, \
except create_schedule, which only proposes a schedule and never saves \
anything. If a tool returns no results, say so. Answer concisely in the \
user's language. When the user's message starts with /schedule they want a \
recurring scheduled task: gather anything unclear first (what to run and how \
often), then call create_schedule once with the final details. The schedule is \
only created after the user confirms the proposal card, so never say it is \
already created.";

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

/// The last `create_schedule` proposal recorded during an agent run, shaped
/// for the FE confirmation card. `None` when the tool was never called.
pub fn pending_action(trace: &ToolTrace) -> Option<Value> {
    let recorded = trace.lock().ok()?;
    recorded
        .iter()
        .rev()
        .find(|call| call.name == CREATE_SCHEDULE_NAME)
        .map(|call| {
            serde_json::json!({
                "kind": "create_schedule",
                "proposal": call.arguments.clone(),
            })
        })
}
