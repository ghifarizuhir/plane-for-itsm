//! `POST /api/workspaces/:slug/ai-agent/` — Rig tool-calling agent prototype.
//!
//! Demo-only surface: no Django counterpart, consumed by the web app's agent
//! mode (`createAgentTask`).
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
use crate::routes::ai_conversations::{
    conversation_gone, finish_turn, insert_message, load_owned_conversation, message_json,
    recent_messages, title_from, ConversationRow, MessageRow,
};
use crate::routes::module::guard_am;
use crate::routes::project::{deny, missing, ws_role};
use crate::{middleware::auth::AuthUser, state::AppState};
use ai::agent::{current_user_context, history_prompt, HistoryMessage, HISTORY_MESSAGE_LIMIT};
use ai::llm::{host_of, resolve_llm_config, LlmError};

pub use ai::agent::{
    new_trace, pending_action, pending_actions, prompt_from_body, run_agent, AGENT_TIMEOUT,
};

// Used by the unit tests below only.
#[cfg(test)]
pub use ai::agent::{effective_prompt, record};
// Used by `crates/api/tests/ai_agent_test.rs`; the bin target sees them as
// unused, so the lint is allowed there.
#[allow(unused_imports)]
pub use ai::agent::{ToolCallTrace, ToolTrace};

pub mod tools {
    pub use ai::tools::*;
}

/// Metadata entry for every `create_work_item` proposal in this turn: one
/// server-generated key per proposal so the FE can persist a decision per card.
pub fn work_item_proposals_metadata(trace: &ToolTrace) -> Vec<Value> {
    trace
        .lock()
        .map(|recorded| {
            recorded
                .iter()
                .filter(|call| call.name == ai::tools::CREATE_WORK_ITEM_NAME)
                .map(|call| json!({"key": Uuid::new_v4(), "proposal": call.arguments.clone()}))
                .collect()
        })
        .unwrap_or_default()
}

/// Metadata entry for every generic mutation proposal in this turn: one
/// server-generated key per proposal so the FE can persist a decision per card.
pub fn proposal_metadata(trace: &ToolTrace) -> Vec<Value> {
    trace
        .lock()
        .map(|recorded| {
            recorded
                .iter()
                .filter_map(|call| {
                    ai::tools::mutation_kind_for_tool(&call.name).map(|kind| {
                        json!({"key": Uuid::new_v4(), "kind": kind, "proposal": call.arguments.clone()})
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 200 response body: raw text, newline-mapped HTML for the chat bubble, the
/// recorded tool calls, the last `create_schedule` proposal (if any), and all
/// proposals of the turn (including `create_work_item`) for the FE cards.
pub fn success_body(
    text: &str,
    tool_calls: Vec<Value>,
    action: Option<Value>,
    pending_actions: Vec<Value>,
) -> Value {
    json!({
        "response": text,
        "response_html": crate::routes::ai::response_html(text),
        "tool_calls": tool_calls,
        "pending_action": action,
        "pending_actions": pending_actions,
    })
}

/// 200 body: the existing chat fields plus the persisted rows so the FE can
/// reconcile its optimistic bubble and refresh the conversation list.
pub(crate) fn chat_success_body(
    text: &str,
    tool_calls: Vec<Value>,
    action: Option<Value>,
    pending_actions: Vec<Value>,
    conversation: &ConversationRow,
    user_message: &MessageRow,
    assistant_message: &MessageRow,
) -> Value {
    let mut body = success_body(text, tool_calls, action, pending_actions);
    body["conversation"] = crate::routes::ai_conversations::conversation_json(conversation);
    body["user_message"] = message_json(user_message);
    body["assistant_message"] = message_json(assistant_message);
    body
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
    let Some(prompt) = prompt_from_body(&body) else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Prompt is required"})),
        ));
    };
    let Some(conversation_id) = body
        .get("conversation_id")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
    else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "conversation_id is required"})),
        ));
    };
    let Some(conversation) =
        load_owned_conversation(&st.pool, &slug, conversation_id, auth.0).await?
    else {
        return Ok(missing());
    };
    if conversation.mode != "agent" {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "conversation mode does not match this endpoint"})),
        ));
    }
    // Dipindah dari atas fungsi (lihat catatan urutan validasi di atas).
    let cfg = resolve_llm_config(&st.pool).await;
    if cfg.api_key.is_empty() || cfg.model.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "AI is not configured for this workspace."})),
        ));
    }
    let task = task_from_body(&body);
    let context = body.get("context").and_then(Value::as_str).unwrap_or("");
    // Name the signed-in user in the prompt so "me"/"saya" resolves to a person
    // without asking the user who they are.
    let current_user: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT display_name, email FROM users WHERE id = $1")
            .bind(auth.0)
            .fetch_optional(&st.pool)
            .await?;
    let context = match current_user {
        Some((display_name, email)) => current_user_context(context, display_name.as_deref(), email.as_deref()),
        None => context.to_string(),
    };
    let history_rows =
        recent_messages(&st.pool, conversation_id, HISTORY_MESSAGE_LIMIT as i64).await?;
    let history: Vec<HistoryMessage> = history_rows
        .iter()
        .map(|row| HistoryMessage {
            role: row.role.clone(),
            content: row.content.clone(),
        })
        .collect();
    let model_prompt = history_prompt(&context, &history, prompt);
    let title = title_from(prompt);
    // Insert the user message on a pooled connection, then release it before
    // the (up to 180s) LLM call so the pool is not held. A conversation that
    // vanished since the load still answers 404, not 500.
    let mut conn = st.pool.acquire().await?;
    let user_message =
        match insert_message(&mut conn, conversation_id, "user", prompt, None, &json!({})).await {
            Ok(message) => message,
            Err(error) if conversation_gone(&error) => return Ok(missing()),
            Err(error) => return Err(error.into()),
        };
    drop(conn);
    let trace = new_trace();
    let tool_server = tools::workspace_tools(st.pool.clone(), workspace_id, auth.0, trace.clone());
    let agent_result = tokio::time::timeout(
        AGENT_TIMEOUT,
        run_agent(
            &cfg.base_url,
            &cfg.api_key,
            &cfg.model,
            tool_server,
            task,
            &model_prompt,
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
            let action = pending_action(&trace);
            let actions = pending_actions(&trace);
            let work_items = work_item_proposals_metadata(&trace);
            let proposals = proposal_metadata(&trace);
            let mut metadata = json!({ "is_error": false });
            if let Some(action) = action.as_ref() {
                metadata["schedule_proposal"] = action["proposal"].clone();
                metadata["schedule_proposal_key"] = json!(Uuid::new_v4());
                metadata["schedule_decision"] = json!("pending");
            }
            if !work_items.is_empty() {
                metadata["work_item_proposals"] = json!(work_items);
            }
            if !proposals.is_empty() {
                metadata["proposals"] = json!(proposals);
            }
            // Assistant message + prune + updated_at in ONE transaction. A
            // conversation deleted mid-turn (row gone / FK violation) → 404.
            let turn = async {
                let mut tx = st.pool.begin().await?;
                let assistant_message = insert_message(
                    &mut tx,
                    conversation_id,
                    "assistant",
                    &text,
                    Some(&crate::routes::ai::response_html(&text)),
                    &metadata,
                )
                .await?;
                let conversation = finish_turn(&mut tx, conversation_id, &title).await?;
                tx.commit().await?;
                Ok::<_, sqlx::Error>((assistant_message, conversation))
            }
            .await;
            let (assistant_message, conversation) = match turn {
                Ok(ok) => ok,
                Err(error) if conversation_gone(&error) => return Ok(missing()),
                Err(error) => return Err(error.into()),
            };
            Ok((
                StatusCode::OK,
                Json(chat_success_body(
                    &text,
                    tool_calls,
                    action,
                    actions,
                    &conversation,
                    &user_message,
                    &assistant_message,
                )),
            ))
        }
        Err(error) => {
            let message = match error {
                LlmError::RateLimited => {
                    format!("Rate limit exceeded for {}", host_of(&cfg.base_url))
                }
                LlmError::Upstream => "An internal error has occurred.".to_string(),
            };
            if let Ok(mut tx) = st.pool.begin().await {
                if let Err(error) = insert_message(
                    &mut tx,
                    conversation_id,
                    "assistant",
                    &message,
                    None,
                    &json!({ "is_error": true }),
                )
                .await
                {
                    tracing::warn!(
                        error=%error,
                        conversation_id=%conversation_id,
                        "failed to persist error turn"
                    );
                } else if let Err(error) = finish_turn(&mut tx, conversation_id, &title).await {
                    tracing::warn!(
                        error=%error,
                        conversation_id=%conversation_id,
                        "failed to finish error turn"
                    );
                }
                let _ = tx.commit().await;
            }
            match error {
                LlmError::RateLimited => Ok((
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(json!({"error": message})),
                )),
                LlmError::Upstream => Ok((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": message})),
                )),
            }
        }
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
        let body = success_body(
            "line1\nline2",
            vec![json!({"name": "list_projects"})],
            None,
            vec![],
        );
        assert_eq!(body["response"], json!("line1\nline2"));
        assert_eq!(body["response_html"], json!("line1<br/>line2"));
        assert_eq!(body["tool_calls"][0]["name"], json!("list_projects"));
        assert_eq!(body["pending_action"], json!(null));
        assert_eq!(body["pending_actions"], json!([]));
    }

    #[test]
    fn success_body_carries_pending_action() {
        let action = json!({"kind": "create_schedule", "proposal": {"frequency": "daily"}});
        let body = success_body("done", vec![], Some(action.clone()), vec![action.clone()]);
        assert_eq!(body["pending_action"], action);
        assert_eq!(body["pending_actions"][0], action);
    }

    #[test]
    fn success_body_carries_work_item_pending_actions() {
        let action = json!({"kind": "create_work_item", "proposal": {"name": "Fix pump"}});
        let body = success_body("done", vec![], None, vec![action.clone()]);
        assert_eq!(body["pending_actions"][0], action);
    }

    #[test]
    fn proposal_metadata_maps_mutation_tools_with_kind_and_key() {
        let trace = new_trace();
        record(
            &trace,
            ai::tools::UPDATE_WORK_ITEM_NAME,
            &json!({"work_item": "LTS-1", "changes": {"priority": "high"}}),
        );
        record(&trace, "search_work_items", &json!({}));
        record(
            &trace,
            ai::tools::ADD_COMMENT_NAME,
            &json!({"work_item": "LTS-1", "comment": "hi"}),
        );
        let proposals = proposal_metadata(&trace);
        assert_eq!(proposals.len(), 2);
        assert_eq!(proposals[0]["kind"], json!("update_work_item"));
        assert_eq!(
            proposals[0]["proposal"]["changes"]["priority"],
            json!("high")
        );
        assert_ne!(proposals[0]["key"], proposals[1]["key"]);
        assert!(proposals[0]["key"].as_str().is_some());
        assert_eq!(proposals[1]["kind"], json!("add_comment"));
    }

    #[test]
    fn work_item_proposals_metadata_assigns_a_key_per_proposal() {
        let trace = new_trace();
        record(
            &trace,
            ai::tools::CREATE_WORK_ITEM_NAME,
            &json!({"name": "Fix pump"}),
        );
        record(
            &trace,
            ai::tools::CREATE_WORK_ITEM_NAME,
            &json!({"name": "Swap filter"}),
        );
        let proposals = work_item_proposals_metadata(&trace);
        assert_eq!(proposals.len(), 2);
        assert_ne!(proposals[0]["key"], proposals[1]["key"]);
        assert_eq!(proposals[0]["proposal"]["name"], json!("Fix pump"));
        assert_eq!(proposals[1]["proposal"]["name"], json!("Swap filter"));
        assert!(proposals[0]["key"].as_str().is_some());
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
