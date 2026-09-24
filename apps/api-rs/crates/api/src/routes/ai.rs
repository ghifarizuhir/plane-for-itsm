//! `POST /api/workspaces/:slug/ai-assistant/` — parity with Django
//! `WorkspaceGPTIntegrationEndpoint` (`plane/app/views/external/base.py:184-212`,
//! `plane/app/urls/external.py:19`).
//!
//! Config resolution mirrors `license/utils/instance_value.py:17-39`:
//! `SKIP_ENV_VAR=1` (default) reads `instance_configurations` (Fernet-decrypt
//! when `is_encrypted`) with per-key env fallback; `SKIP_ENV_VAR=0` reads env
//! directly. `LLM_BASE_URL` is env-only (design 2026-09-21).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};

pub use ai::llm::{
    host_of, llm_config_from_env, llm_config_from_rows, resolve_llm_config, response_html,
    LlmConfig, LlmError, DEFAULT_BASE_URL, DEFAULT_MODEL,
};

use crate::routes::module::guard_am;
use crate::routes::project::{deny, ws_role};
use crate::{middleware::auth::AuthUser, state::AppState};

/// `{base_url}/chat/completions` with exactly one joining slash.
pub fn chat_url(base_url: &str) -> String {
    format!("{}/chat/completions", base_url.trim_end_matches('/'))
}

/// OpenAI-compatible chat body; Django concatenates `task + "\n" + prompt`
/// (`views/external/base.py:125`).
pub fn build_body(model: &str, task: &str, prompt: &str) -> Value {
    json!({
        "model": model,
        "messages": [{"role": "user", "content": format!("{task}\n{prompt}")}],
    })
}

/// `choices[0].message.content` as a string; anything missing → `""`.
pub fn extract_content(v: &Value) -> String {
    v.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string()
}

fn http() -> &'static reqwest::Client {
    static HTTP: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    HTTP.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("ai client")
    })
}

/// POST `{base_url}/chat/completions` and return the assistant text.
/// 429 → `RateLimited`; every other failure → `Upstream`. The API key is
/// never logged.
pub async fn chat_completion(
    base_url: &str,
    api_key: &str,
    model: &str,
    task: &str,
    prompt: &str,
) -> Result<String, LlmError> {
    let resp = http()
        .post(chat_url(base_url))
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&build_body(model, task, prompt))
        .send()
        .await
        .map_err(|e| {
            tracing::warn!(error=%e, "ai: upstream request failed");
            LlmError::Upstream
        })?;
    let status = resp.status();
    if status.as_u16() == 429 {
        return Err(LlmError::RateLimited);
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(500).collect();
        tracing::warn!(status = status.as_u16(), body = %snippet, "ai: upstream error");
        return Err(LlmError::Upstream);
    }
    let value: Value = resp.json().await.map_err(|e| {
        tracing::warn!(error=%e, "ai: upstream returned invalid json");
        LlmError::Upstream
    })?;
    Ok(extract_content(&value))
}

/// Django `if not request.data.get("task", False)` (`base.py:159-161`).
pub fn task_from_body(body: &Value) -> Option<&str> {
    body.get("task")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

pub async fn workspace_ai_assistant(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let role = ws_role(&st.pool, auth.0, &slug).await?;
    if guard_am(role).is_err() {
        return Ok(deny());
    }
    let cfg = resolve_llm_config(&st.pool).await;
    if cfg.api_key.is_empty() || cfg.model.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "AI is not configured for this workspace."})),
        ));
    }
    let Some(task) = task_from_body(&body) else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Task is required"})),
        ));
    };
    let prompt = body.get("prompt").and_then(Value::as_str).unwrap_or("");
    match chat_completion(&cfg.base_url, &cfg.api_key, &cfg.model, task, prompt).await {
        Ok(text) => {
            let html = response_html(&text);
            Ok((
                StatusCode::OK,
                Json(json!({"response": text, "response_html": html})),
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
    fn chat_url_strips_trailing_slash() {
        assert_eq!(
            chat_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            chat_url("http://localhost:11434/v1"),
            "http://localhost:11434/v1/chat/completions"
        );
    }

    #[test]
    fn build_body_joins_task_and_prompt() {
        let body = build_body("gpt-4o-mini", "do it", "text");
        assert_eq!(body["model"], json!("gpt-4o-mini"));
        assert_eq!(body["messages"][0]["role"], json!("user"));
        assert_eq!(body["messages"][0]["content"], json!("do it\ntext"));
    }

    #[test]
    fn extract_content_handles_missing_and_empty() {
        assert_eq!(
            extract_content(&json!({"choices": [{"message": {"content": "hi"}}]})),
            "hi"
        );
        assert_eq!(extract_content(&json!({"choices": []})), "");
        assert_eq!(
            extract_content(&json!({"choices": [{"message": {"content": null}}]})),
            ""
        );
    }

    #[test]
    fn response_html_maps_newlines_only() {
        assert_eq!(response_html("a\nb"), "a<br/>b");
        assert_eq!(response_html("plain"), "plain");
        assert_eq!(response_html(""), "");
    }

    #[test]
    fn config_encrypted_row_is_decrypted() {
        let rows = vec![("LLM_API_KEY".to_string(), Some("enc".to_string()), true)];
        let cfg = llm_config_from_rows(&rows, "env-key".into(), String::new(), None, |v| {
            format!("dec:{v}")
        });
        assert_eq!(cfg.api_key, "dec:enc");
    }

    #[test]
    fn config_plain_row_is_used_as_is() {
        let rows = vec![("LLM_API_KEY".to_string(), Some("plain".to_string()), false)];
        let cfg = llm_config_from_rows(&rows, "env-key".into(), String::new(), None, |_| {
            String::new()
        });
        assert_eq!(cfg.api_key, "plain");
    }

    #[test]
    fn config_missing_row_falls_back_to_env() {
        let cfg = llm_config_from_rows(&[], "env-key".into(), "env-model".into(), None, |_| {
            String::new()
        });
        assert_eq!(cfg.api_key, "env-key");
        assert_eq!(cfg.model, "env-model");
    }

    #[test]
    fn config_null_model_uses_default() {
        let rows = vec![("LLM_MODEL".to_string(), None, false)];
        let cfg =
            llm_config_from_rows(&rows, String::new(), String::new(), None, |_| String::new());
        assert_eq!(cfg.model, DEFAULT_MODEL);
    }

    #[test]
    fn config_empty_env_model_uses_default() {
        let cfg = llm_config_from_rows(&[], String::new(), "  ".into(), None, |_| String::new());
        assert_eq!(cfg.model, DEFAULT_MODEL);
    }

    #[test]
    fn config_base_url_defaults_and_overrides() {
        let cfg = llm_config_from_rows(&[], String::new(), String::new(), None, |_| String::new());
        assert_eq!(cfg.base_url, DEFAULT_BASE_URL);
        let cfg = llm_config_from_rows(
            &[],
            String::new(),
            String::new(),
            Some("http://localhost:11434/v1".into()),
            |_| String::new(),
        );
        assert_eq!(cfg.base_url, "http://localhost:11434/v1");
        let cfg =
            llm_config_from_rows(&[], String::new(), String::new(), Some("  ".into()), |_| {
                String::new()
            });
        assert_eq!(cfg.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn env_reader_reflects_process_env() {
        let (key, model, base) = llm_config_from_env();
        assert_eq!(key, std::env::var("LLM_API_KEY").unwrap_or_default());
        assert_eq!(model, std::env::var("LLM_MODEL").unwrap_or_default());
        assert_eq!(base, std::env::var("LLM_BASE_URL").ok());
    }

    #[test]
    fn skip_env_path_resolves_from_env_only() {
        // SKIP_ENV_VAR=0 composition: empty rows → pure env resolution.
        let (key, model, base) = llm_config_from_env();
        let cfg = llm_config_from_rows(&[], key.clone(), model.clone(), base.clone(), |_| {
            String::new()
        });
        assert_eq!(cfg.api_key, key.trim());
        let expected_model = if model.trim().is_empty() {
            DEFAULT_MODEL.to_string()
        } else {
            model.trim().to_string()
        };
        assert_eq!(cfg.model, expected_model);
    }

    #[test]
    fn task_from_body_rules() {
        assert_eq!(task_from_body(&json!({"task": "hi"})), Some("hi"));
        assert_eq!(task_from_body(&json!({})), None);
        assert_eq!(task_from_body(&json!({"task": ""})), None);
        assert_eq!(task_from_body(&json!({"task": 5})), None);
        assert_eq!(task_from_body(&json!({"task": null})), None);
    }
}
