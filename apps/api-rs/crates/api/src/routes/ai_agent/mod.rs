//! `POST /api/workspaces/:slug/ai-agent/` — Rig tool-calling agent prototype.
//!
//! Demo-only surface: no Django counterpart and not consumed by the web app.
//! Runs an OpenAI-compatible chat-completions agent (Rig
//! `openai::CompletionsClient`) over typed, workspace-scoped read-only tools.
//! The `/ai-assistant/` parity contract is untouched.
//!
//! Rig 0.42 uses reqwest 0.13, the repo uses reqwest 0.12 — always pass
//! `rig::http_client::ReqwestClient`, never `reqwest::Client`.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::routes::ai::task_from_body;
use crate::routes::module::guard_am;
use crate::routes::project::{deny, ws_role};
use crate::{middleware::auth::AuthUser, state::AppState};
use ai::llm::{host_of, resolve_llm_config, LlmError};

pub use ai::agent::{
    effective_prompt, new_trace, pending_action, prompt_from_body, record, run_agent,
    ToolCallTrace, ToolTrace, AGENT_TIMEOUT, MAX_TURNS, PREAMBLE,
};
pub use ai::tools::{workspace_tools, CreateSchedule, CreateScheduleArgs};

pub mod tools {
    pub use ai::tools::*;
}

/// 200 response body: raw text, newline-mapped HTML for the chat bubble, and
/// the recorded tool calls.
pub fn success_body(text: &str, tool_calls: Vec<Value>) -> Value {
    json!({
        "response": text,
        "response_html": crate::routes::ai::response_html(text),
        "tool_calls": tool_calls,
    })
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
            Ok((StatusCode::OK, Json(success_body(&text, tool_calls))))
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
    fn success_body_maps_newlines_and_keeps_tool_calls() {
        let body = success_body("line1\nline2", vec![json!({"name": "list_projects"})]);
        assert_eq!(body["response"], json!("line1\nline2"));
        assert_eq!(body["response_html"], json!("line1<br/>line2"));
        assert_eq!(body["tool_calls"][0]["name"], json!("list_projects"));
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
