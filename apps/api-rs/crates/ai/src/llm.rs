//! LLM config resolution (parity with `routes/ai.rs`) + shared helpers.

use sqlx::PgPool;

use common::crypto::{decrypt_data, fernet_secret, skip_env_vars};

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_MODEL: &str = "gpt-4o-mini";

#[derive(Clone, PartialEq)]
pub struct LlmConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

impl std::fmt::Debug for LlmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmConfig")
            .field("api_key", &"<redacted>")
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .finish()
    }
}

/// Pure mapping from raw `(key, value, is_encrypted)` rows + env fallbacks.
/// A present row masks the env default (Django `instance_value.py:23-33`),
/// even when its value is NULL (→ empty string).
pub fn llm_config_from_rows(
    rows: &[(String, Option<String>, bool)],
    env_api_key: String,
    env_model: String,
    env_base_url: Option<String>,
    decrypt: impl Fn(&str) -> String,
) -> LlmConfig {
    let api_key = match rows.iter().find(|(k, _, _)| k == "LLM_API_KEY") {
        Some((_, value, is_encrypted)) => {
            let raw = value.clone().unwrap_or_default();
            if *is_encrypted {
                decrypt(&raw)
            } else {
                raw
            }
        }
        None => env_api_key,
    };
    let model = match rows.iter().find(|(k, _, _)| k == "LLM_MODEL") {
        Some((_, Some(value), _)) if !value.trim().is_empty() => value.clone(),
        _ if !env_model.trim().is_empty() => env_model,
        _ => DEFAULT_MODEL.to_string(),
    };
    let base_url = env_base_url
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    LlmConfig {
        api_key: api_key.trim().to_string(),
        model: model.trim().to_string(),
        base_url,
    }
}

/// Pure env reads so the `SKIP_ENV_VAR=0` wiring is unit-testable
/// (process env itself can't be safely mutated in parallel tests).
pub fn llm_config_from_env() -> (String, String, Option<String>) {
    (
        std::env::var("LLM_API_KEY").unwrap_or_default(),
        std::env::var("LLM_MODEL").unwrap_or_default(),
        std::env::var("LLM_BASE_URL").ok(),
    )
}

/// Resolve the effective AI config from DB or env per `SKIP_ENV_VAR`.
pub async fn resolve_llm_config(pool: &PgPool) -> LlmConfig {
    let (env_api_key, env_model, env_base_url) = llm_config_from_env();
    if !skip_env_vars() {
        return llm_config_from_rows(&[], env_api_key, env_model, env_base_url, |_| String::new());
    }
    let rows: Vec<(String, Option<String>, bool)> = sqlx::query_as(
        "SELECT key, value, is_encrypted FROM instance_configurations \
         WHERE key IN ('LLM_API_KEY','LLM_MODEL') AND deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::warn!(error=%e, "ai: instance_configurations lookup failed");
        e
    })
    .unwrap_or_default();
    let secret = fernet_secret();
    llm_config_from_rows(&rows, env_api_key, env_model, env_base_url, |v| {
        decrypt_data(v, &secret)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmError {
    RateLimited,
    Upstream,
}

/// Django parity: `/ai-assistant/` returns the raw text plus a copy with
/// newlines mapped to `<br/>`. Shared with the agent route so both chat modes
/// render identically.
pub fn response_html(text: &str) -> String {
    text.replace('\n', "<br/>")
}

/// `host` for the 429 body, e.g. `api.openai.com`.
pub fn host_of(base_url: &str) -> String {
    base_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("")
        .to_string()
}
