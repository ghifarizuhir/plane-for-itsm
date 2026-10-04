//! Rig tool-calling agent runtime, shared by the HTTP handler and scheduled
//! runs.

use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rig::completion::PromptError;
use rig::prelude::*;
use rig::providers::openai;
use rig::tool::server::ToolServerHandle;
use serde_json::{json, Value};

use crate::llm::LlmError;
use crate::tools::{CREATE_SCHEDULE_NAME, CREATE_WORK_ITEM_NAME};

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
Answer factual questions about projects, work items, people, and their \
metadata by calling the provided tools; never invent identifiers, names, \
counts, or states. All tools are scoped to the user's current workspace and \
read-only, except create_schedule and create_work_item, which only propose \
something and never save anything. If a tool returns no results, say so. \
Answer concisely in the user's language. Use get_work_item for one work \
item's details, list_work_item_comments for its discussion, and \
list_work_item_relations for blockers. search_work_items and \
count_work_items can filter by service, type, assignee (a name, email, or \
\"me\"), sprint, track, and label; use assignee \"me\" for the user's own \
items. Use list_members, list_states, list_labels, and list_work_item_types \
to resolve names before answering or proposing changes. When the user's \
message starts with /schedule they want a recurring scheduled task. A \
schedule is a recipe, not a one-line command: gather anything unclear first, \
then call create_schedule once with a complete recipe — description, ordered \
how_to steps, at least one read tool, expected_output, and how often. Tell \
the user they can edit every field in the confirmation card. The schedule is \
only created after the user confirms the proposal card, so never say it is \
already created. When the user clearly asks to create a work item or task \
(natural language or a message starting with /task), propose exactly one work \
item per create_work_item call. The project must be named by the user: ask \
when it is missing or ambiguous, and never guess. State names, assignee names \
or emails, and label names may be human-readable; the UI resolves them. The \
context may name the signed-in user as Current user; when the user refers to \
themselves (\"me\", \"saya\"), that is the person to use as the assignee. A \
work item is only created after the user confirms the proposal card, so never \
say it is already created.";

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

/// How many stored messages are folded back into the model prompt.
pub const HISTORY_MESSAGE_LIMIT: usize = 8;

/// One stored conversation message used to rebuild model context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
}

/// Append the signed-in user's identity to the FE-supplied context block so the
/// model can resolve first-person references ("me", "saya") to a person. Blank
/// parts are ignored; with no usable identity the context is unchanged.
pub fn current_user_context(context: &str, display_name: Option<&str>, email: Option<&str>) -> String {
    let name = display_name.map(str::trim).filter(|part| !part.is_empty());
    let email = email.map(str::trim).filter(|part| !part.is_empty());
    let identity = match (name, email) {
        (Some(name), Some(email)) => format!("{name} <{email}>"),
        (Some(name), None) => name.to_string(),
        (None, Some(email)) => email.to_string(),
        (None, None) => return context.to_string(),
    };
    let line = format!("Current user: {identity}");
    let context = context.trim();
    if context.is_empty() {
        line
    } else {
        format!("{context}\n{line}")
    }
}

/// Compose the model prompt from the FE-supplied context block, the stored
/// conversation (newest `HISTORY_MESSAGE_LIMIT`, oldest first) and the new
/// question. Mirrors the old FE `buildAiPrompt` output.
pub fn history_prompt(context: &str, history: &[HistoryMessage], question: &str) -> String {
    let history_block = history
        .iter()
        .rev()
        .take(HISTORY_MESSAGE_LIMIT)
        .rev()
        .map(|message| {
            let role = if message.role == "user" {
                "User"
            } else {
                "Assistant"
            };
            format!("{role}: {}", message.content)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let history_block = if history_block.is_empty() {
        "(empty)".to_string()
    } else {
        history_block
    };
    let context = context.trim();
    let prefix = if context.is_empty() {
        String::new()
    } else {
        format!("{context}\n\n")
    };
    format!("{prefix}Conversation so far:\n{history_block}\n\nUser's new question: {question}")
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

/// All proposal tool calls recorded during an agent run, in call order, shaped
/// for the FE confirmation cards. Covers `create_schedule` and
/// `create_work_item`; read-only calls are ignored.
pub fn pending_actions(trace: &ToolTrace) -> Vec<Value> {
    let Ok(recorded) = trace.lock() else {
        return Vec::new();
    };
    recorded
        .iter()
        .filter(|call| call.name == CREATE_SCHEDULE_NAME || call.name == CREATE_WORK_ITEM_NAME)
        .map(|call| {
            json!({
                "kind": call.name,
                "proposal": call.arguments.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pending_actions_returns_all_proposals_in_order() {
        let trace = new_trace();
        record(&trace, "list_projects", &json!({}));
        record(&trace, CREATE_SCHEDULE_NAME, &json!({"name": "Daily"}));
        record(&trace, CREATE_WORK_ITEM_NAME, &json!({"name": "Fix pump"}));
        let actions = pending_actions(&trace);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0]["kind"], json!("create_schedule"));
        assert_eq!(actions[0]["proposal"]["name"], json!("Daily"));
        assert_eq!(actions[1]["kind"], json!("create_work_item"));
        assert_eq!(actions[1]["proposal"]["name"], json!("Fix pump"));
    }

    #[test]
    fn current_user_context_appends_identity_to_existing_context() {
        let out = current_user_context(
            "Work item context:\nWork item: X",
            Some("Ghifari"),
            Some("ghifari@example.com"),
        );
        assert_eq!(
            out,
            "Work item context:\nWork item: X\nCurrent user: Ghifari <ghifari@example.com>"
        );
    }

    #[test]
    fn current_user_context_uses_whichever_identity_part_is_present() {
        assert_eq!(
            current_user_context("", Some("Ghifari"), None),
            "Current user: Ghifari"
        );
        assert_eq!(
            current_user_context("ctx", None, Some("ghifari@example.com")),
            "ctx\nCurrent user: ghifari@example.com"
        );
    }

    #[test]
    fn current_user_context_without_identity_is_unchanged() {
        assert_eq!(current_user_context("ctx", None, None), "ctx");
        assert_eq!(current_user_context("ctx", Some("  "), Some("")), "ctx");
        assert_eq!(current_user_context("", None, None), "");
    }

    #[test]
    fn pending_action_keeps_returning_the_last_schedule() {
        let trace = new_trace();
        record(&trace, CREATE_SCHEDULE_NAME, &json!({"name": "First"}));
        record(&trace, CREATE_WORK_ITEM_NAME, &json!({"name": "Task"}));
        record(&trace, CREATE_SCHEDULE_NAME, &json!({"name": "Second"}));
        let action = pending_action(&trace).expect("schedule action");
        assert_eq!(action["kind"], json!("create_schedule"));
        assert_eq!(action["proposal"]["name"], json!("Second"));
    }
}
