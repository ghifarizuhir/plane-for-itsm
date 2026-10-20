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

/// Render model output (GitHub-flavored Markdown) as HTML for the clients.
///
/// Raw HTML is escaped (`render.escape`) so the fragment only contains tags the
/// renderer emits; the web app still allowlist-sanitizes before injection. Soft
/// breaks stay `<br />`, preserving the plain-text newline behavior the Django
/// endpoint had. Shared by chat, editor completion, and scheduled runs.
pub fn response_html(text: &str) -> String {
    let mut options = comrak::Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.render.hardbreaks = true;
    options.render.escape = true;
    comrak::markdown_to_html(text, &options)
        .trim_end()
        .to_string()
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

#[cfg(test)]
mod tests {
    use super::response_html;

    #[test]
    fn response_html_empty_is_empty() {
        assert_eq!(response_html(""), "");
    }

    #[test]
    fn response_html_renders_paragraphs_and_soft_breaks() {
        assert_eq!(response_html("plain"), "<p>plain</p>");
        assert_eq!(response_html("a\nb"), "<p>a<br />\nb</p>");
        assert_eq!(response_html("a\n\nb"), "<p>a</p>\n<p>b</p>");
    }

    #[test]
    fn response_html_renders_inline_markdown() {
        assert!(response_html("**bold**").contains("<strong>bold</strong>"));
        assert!(response_html("`code`").contains("<code>code</code>"));
        assert!(response_html("[x](https://example.com)")
            .contains("<a href=\"https://example.com\">x</a>"));
    }

    #[test]
    fn response_html_renders_blocks_and_gfm_tables() {
        assert!(response_html("## Title").contains("<h2>Title</h2>"));
        assert!(response_html("- one\n- two").contains("<li>one</li>"));
        assert!(response_html("1. one\n2. two").contains("<ol>"));
        assert!(response_html("> quote").contains("<blockquote>"));
        assert!(response_html("```\ncode\n```").contains("<pre><code>code"));
        assert!(response_html("| a | b |\n| - | - |\n| 1 | 2 |").contains("<table>"));
    }

    #[test]
    fn response_html_escapes_raw_html() {
        assert_eq!(
            response_html("<script>alert(1)</script>"),
            "&lt;script&gt;alert(1)&lt;/script&gt;"
        );
        assert_eq!(
            response_html("a <b>bold</b>"),
            "<p>a &lt;b&gt;bold&lt;/b&gt;</p>"
        );
    }
}
