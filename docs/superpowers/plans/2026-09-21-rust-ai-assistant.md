# Rust AI Assistant Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Serve `POST /api/workspaces/:slug/ai-assistant/` from Rust (OpenAI-compatible, configurable base URL) so the web app's AI features work again in the Rust-only prod.

**Architecture:** New `routes/ai.rs` module holds config resolution (DB `instance_configurations` with env fallback + Fernet decrypt, mirroring Django `instance_value.py`), a pure-ish `chat_completion()` that POSTs to `{LLM_BASE_URL}/chat/completions`, and the axum handler gated by workspace ADMIN/MEMBER. `instance.rs` reports `has_llm_configured` from the same resolver. Inventory gets a new `ai` domain and an ADR records the deliberate deltas.

**Tech Stack:** Rust 1.96 (axum 0.7, sqlx 0.7 Postgres, reqwest 0.12 rustls), parity gates (`route_inventory_test`, `fe_tripwire_test`), `apps/api-rs/scripts/smoke.sh`.

**Spec:** `docs/superpowers/specs/2026-09-21-rust-ai-assistant-design.md`

---

## File map

| Action | File                                                                                    | Responsibility                                          |
| ------ | --------------------------------------------------------------------------------------- | ------------------------------------------------------- |
| Create | `apps/api-rs/crates/api/src/routes/ai.rs`                                               | Config resolution, upstream client, handler, unit tests |
| Create | `apps/api-rs/crates/api/tests/ai_test.rs`                                               | Fake-upstream integration tests                         |
| Create | `docs/superpowers/decisions/2026-09-21-ai-assistant-rust.md`                            | ADR for the deviation                                   |
| Modify | `apps/api-rs/crates/api/src/routes/mod.rs`                                              | Register `pub mod ai;`                                  |
| Modify | `apps/api-rs/crates/api/src/main.rs`                                                    | Route `POST /api/workspaces/:slug/ai-assistant/`        |
| Modify | `apps/api-rs/crates/api/src/routes/instance_admin.rs`                                   | `fernet_secret` → `pub(crate)`                          |
| Modify | `apps/api-rs/crates/api/src/routes/instance.rs`                                         | DB-aware `has_llm_configured`                           |
| Modify | `apps/api-rs/crates/api/parity-inventory.json`                                          | New `ai` domain; workspace entry `deviation_accepted`   |
| Modify | `docs/superpowers/decisions/2026-09-10-f0-stays-on-django.md`                           | Mark GPT section superseded (workspace only)            |
| Modify | `apps/api-rs/crates/api/src/routes/search.rs`                                           | Stale comment                                           |
| Modify | `docker-compose.yml`, `docs/design/01-architecture.md`, `docs/business-capabilities.md` | Stale boundary lists                                    |
| Modify | `apps/api-rs/scripts/smoke.sh`                                                          | AI route not-404 check                                  |

---

### Task 1: `routes/ai.rs` — pure helpers + config resolution

**Files:**

- Create: `apps/api-rs/crates/api/src/routes/ai.rs`
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs`
- Modify: `apps/api-rs/crates/api/src/routes/instance_admin.rs:1231` (`fernet_secret` visibility)

- [ ] **Step 1: Register the module**

Add to `apps/api-rs/crates/api/src/routes/mod.rs` (keep alphabetical-ish placement near the top, e.g. after `pub mod analytic;`):

```rust
pub mod ai;
```

- [ ] **Step 2: Write the failing unit tests**

Create `apps/api-rs/crates/api/src/routes/ai.rs` with ONLY the test module (no implementation yet):

```rust
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
        let cfg = llm_config_from_rows(&rows, "env-key".into(), String::new(), None, |_| String::new());
        assert_eq!(cfg.api_key, "plain");
    }

    #[test]
    fn config_missing_row_falls_back_to_env() {
        let cfg = llm_config_from_rows(&[], "env-key".into(), "env-model".into(), None, |_| String::new());
        assert_eq!(cfg.api_key, "env-key");
        assert_eq!(cfg.model, "env-model");
    }

    #[test]
    fn config_null_model_uses_default() {
        let rows = vec![("LLM_MODEL".to_string(), None, false)];
        let cfg = llm_config_from_rows(&rows, String::new(), String::new(), None, |_| String::new());
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
        let cfg = llm_config_from_rows(&[], String::new(), String::new(), Some("  ".into()), |_| String::new());
        assert_eq!(cfg.base_url, DEFAULT_BASE_URL);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p api --lib routes::ai` (workdir `apps/api-rs`)

Expected: compile error — `cannot find function chat_url`, `DEFAULT_MODEL`, etc.

- [ ] **Step 4: Implement the helpers**

Replace the file content with (tests stay at the bottom):

```rust
//! `POST /api/workspaces/:slug/ai-assistant/` — parity with Django
//! `WorkspaceGPTIntegrationEndpoint` (`plane/app/views/external/base.py:184-212`,
//! `plane/app/urls/external.py:19`).
//!
//! Config resolution mirrors `license/utils/instance_value.py:17-39`:
//! `SKIP_ENV_VAR=1` (default) reads `instance_configurations` (Fernet-decrypt
//! when `is_encrypted`) with per-key env fallback; `SKIP_ENV_VAR=0` reads env
//! directly. `LLM_BASE_URL` is env-only (design 2026-09-21).

use serde_json::{json, Value};

use crate::routes::instance_admin::{decrypt_data, fernet_secret, skip_env_vars};

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_MODEL: &str = "gpt-4o-mini";

#[derive(Debug, Clone, PartialEq)]
pub struct LlmConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
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
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    LlmConfig {
        api_key: api_key.trim().to_string(),
        model: model.trim().to_string(),
        base_url,
    }
}

/// Resolve the effective AI config from DB or env per `SKIP_ENV_VAR`.
pub async fn resolve_llm_config(pool: &sqlx::PgPool) -> LlmConfig {
    let env_api_key = std::env::var("LLM_API_KEY").unwrap_or_default();
    let env_model = std::env::var("LLM_MODEL").unwrap_or_default();
    let env_base_url = std::env::var("LLM_BASE_URL").ok();
    if !skip_env_vars() {
        return llm_config_from_rows(&[], env_api_key, env_model, env_base_url, |_| String::new());
    }
    let rows: Vec<(String, Option<String>, bool)> = sqlx::query_as(
        "SELECT key, value, is_encrypted FROM instance_configurations \
         WHERE key IN ('LLM_API_KEY','LLM_MODEL') AND deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let secret = fernet_secret();
    llm_config_from_rows(&rows, env_api_key, env_model, env_base_url, |v| {
        decrypt_data(v, &secret)
    })
}

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
```

- [ ] **Step 5: Expose `fernet_secret` to the module**

In `apps/api-rs/crates/api/src/routes/instance_admin.rs:1231`, change:

```rust
fn fernet_secret() -> String {
```

to:

```rust
pub(crate) fn fernet_secret() -> String {
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p api --lib routes::ai` (workdir `apps/api-rs`)

Expected: PASS (9 tests).

- [ ] **Step 7: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai.rs apps/api-rs/crates/api/src/routes/mod.rs apps/api-rs/crates/api/src/routes/instance_admin.rs
git commit -m "feat(api-rs): ai config resolution and request helpers"
```

---

### Task 2: `chat_completion` — upstream call + fake-server integration tests

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai.rs`
- Create: `apps/api-rs/crates/api/tests/ai_test.rs`

- [ ] **Step 1: Write the failing integration test**

Create `apps/api-rs/crates/api/tests/ai_test.rs`:

```rust
//! Fake OpenAI-compatible upstream for `routes::ai::chat_completion`:
//! success, 429, 5xx, malformed JSON, empty content. No DB, no env.

use api::routes::ai::{chat_completion, LlmError};
use axum::{
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};

fn fake_upstream(status: u16, body: Value) -> Router {
    let handler = move || {
        let body = body.clone();
        async move {
            let code = StatusCode::from_u16(status).unwrap();
            (code, Json(body))
        }
    };
    Router::new().route("/v1/chat/completions", post(handler))
}

async fn spawn(status: u16, body: Value) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, fake_upstream(status, body)).await.unwrap();
    });
    format!("http://{addr}/v1")
}

#[tokio::test]
async fn success_returns_content() {
    let base = spawn(200, json!({"choices": [{"message": {"content": "hello"}}]})).await;
    let out = chat_completion(&base, "key", "gpt-4o-mini", "task", "prompt").await;
    assert_eq!(out, Ok("hello".to_string()));
}

#[tokio::test]
async fn empty_content_is_ok_empty_string() {
    let base = spawn(200, json!({"choices": []})).await;
    let out = chat_completion(&base, "key", "gpt-4o-mini", "task", "prompt").await;
    assert_eq!(out, Ok(String::new()));
}

#[tokio::test]
async fn upstream_429_maps_to_rate_limited() {
    let base = spawn(429, json!({"error": "slow down"})).await;
    let out = chat_completion(&base, "key", "gpt-4o-mini", "task", "prompt").await;
    assert_eq!(out, Err(LlmError::RateLimited));
}

#[tokio::test]
async fn upstream_500_maps_to_upstream_error() {
    let base = spawn(500, json!({"error": "boom"})).await;
    let out = chat_completion(&base, "key", "gpt-4o-mini", "task", "prompt").await;
    assert_eq!(out, Err(LlmError::Upstream));
}

#[tokio::test]
async fn malformed_json_maps_to_upstream_error() {
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
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let base = format!("http://{addr}/v1");
    let out = chat_completion(&base, "key", "gpt-4o-mini", "task", "prompt").await;
    assert_eq!(out, Err(LlmError::Upstream));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --test ai_test` (workdir `apps/api-rs`)

Expected: compile error — `cannot find function chat_completion`, `LlmError`.

- [ ] **Step 3: Implement `chat_completion`**

Append to `apps/api-rs/crates/api/src/routes/ai.rs` (before the test module):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmError {
    RateLimited,
    Upstream,
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p api --test ai_test` (workdir `apps/api-rs`)

Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai.rs apps/api-rs/crates/api/tests/ai_test.rs
git commit -m "feat(api-rs): openai-compatible chat completion with fake-upstream tests"
```

---

### Task 3: Handler + route + inventory/ADR

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/ai.rs`
- Modify: `apps/api-rs/crates/api/src/main.rs` (near the `/api/unsplash/` route)
- Modify: `apps/api-rs/crates/api/parity-inventory.json` (remove 2 entries from `external`, add `ai` domain)
- Create: `docs/superpowers/decisions/2026-09-21-ai-assistant-rust.md`

- [ ] **Step 1: Write the failing handler unit test**

Add to the `#[cfg(test)]` module in `apps/api-rs/crates/api/src/routes/ai.rs`:

```rust
    #[test]
    fn task_from_body_rules() {
        assert_eq!(task_from_body(&json!({"task": "hi"})), Some("hi"));
        assert_eq!(task_from_body(&json!({"task": ""})), None);
        assert_eq!(task_from_body(&json!({})), None);
        assert_eq!(task_from_body(&json!({"task": 5})), None);
        assert_eq!(task_from_body(&json!({"task": null})), None);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib routes::ai` (workdir `apps/api-rs`)

Expected: compile error — `cannot find function task_from_body`.

- [ ] **Step 3: Implement the handler**

In `apps/api-rs/crates/api/src/routes/ai.rs`, extend the import block at the top to:

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};

use crate::routes::instance_admin::{decrypt_data, fernet_secret, skip_env_vars};
use crate::routes::module::guard_am;
use crate::routes::project::{deny, ws_role};
use crate::{middleware::auth::AuthUser, state::AppState};
```

Then append (before the test module):

```rust
/// Django `if not request.data.get("task", False)` (`base.py:159-161`).
pub fn task_from_body(body: &Value) -> Option<&str> {
    body.get("task").and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// `host` for the 429 body, e.g. `api.openai.com`.
fn host_of(base_url: &str) -> String {
    base_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("")
        .to_string()
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
            Json(json!({"error": "LLM provider API key and model are required"})),
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
            let response_html = text.replace('\n', "<br/>");
            Ok((
                StatusCode::OK,
                Json(json!({"response": text, "response_html": response_html})),
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

- [ ] **Step 4: Run unit tests to verify they pass**

Run: `cargo test -p api --lib routes::ai` (workdir `apps/api-rs`)

Expected: PASS (10 tests).

- [ ] **Step 5: Register the route**

In `apps/api-rs/crates/api/src/main.rs`, find the `/api/unsplash/` route (search for `"Parity with \`UnsplashEndpoint\`"`). Add immediately after that `.route(...)` block:

```rust
        // Parity with `WorkspaceGPTIntegrationEndpoint`
        // (`views/external/base.py:184-212`, `urls/external.py:19`): POST
        // 200 `{response, response_html}`; 400 config-missing / `Task is
        // required`; 500 generic upstream; 429 passthrough. Gate WORKSPACE
        // ADMIN/MEMBER. OpenAI-compatible via `LLM_BASE_URL` (env).
        .route(
            "/api/workspaces/:slug/ai-assistant/",
            post(routes::ai::workspace_ai_assistant),
        )
```

- [ ] **Step 6: Write the ADR**

Create `docs/superpowers/decisions/2026-09-21-ai-assistant-rust.md`:

```markdown
# ADR AI-1: workspace ai-assistant pindah ke Rust (deviation_accepted)

Date: 2026-09-21
Status: accepted
Format: docs/superpowers/decisions/2026-09-10-f0-parity-decision-format.md
Supersedes: ADR F0-3 (bagian GPT) untuk endpoint workspace — project-level
`ai-assistant` tetap `stays_on_django`.

## Context

`POST /api/workspaces/:slug/ai-assistant/` (`views/external/base.py:184-212`)
sebelumnya `stays_on_django` karena dianggap "external LLM proxy out of scope"
(`search.rs:22`). Setelah cutover Rust, Django tidak ada di jalur request
(Caddy `/api/*` → `api:8000`), jadi endpoint 404 dan fitur AI web mati.

## Decision

Bangun handler Rust (`routes/ai.rs::workspace_ai_assistant`) dengan kontrak
status/body 1:1 Django (400 config/task, 500 generik, 200
`{response, response_html}`), plus deviasi sengaja:

- Provider model OpenAI-compatible: `POST {LLM_BASE_URL}/chat/completions`,
  `LLM_BASE_URL` env-only default `https://api.openai.com/v1`. `LLM_PROVIDER`
  tidak dibaca; allowlist model Django dihapus (model self-hosted/custom).
- Upstream 429 → HTTP 429 `{"error": "Rate limit exceeded for <host>"}`
  (Django menelan semua error upstream jadi 500 generik; FE sudah punya toast
  429 khusus).
- `prompt` hilang/non-string → `""` dan `task` non-string → 400
  `Task is required` (Django TypeError → 500).
- Body JSON invalid → body 400 axum (Django body parser DRF).
- Tanpa rate limit bulanan per user (tidak ada di Django OSS).

`has_llm_configured` di `/api/instances/` ikut diperbaiki jadi DB-aware
(sebelumnya env-only) supaya key yang disimpan admin AI form menyalakan gate FE.

## Consequences

- Inventory entry: `deviation_accepted` (rust_handler + ADR + notes DECISION).
- Project-level `ai-assistant` tetap `stays_on_django` (zero FE caller).
- `rephrase-grammar` (FE editor AI) tetap tidak dibangun.
- Rollback: revert kode; key DB tidak berbahaya.
```

- [ ] **Step 7: Update the inventory**

In `apps/api-rs/crates/api/parity-inventory.json`, delete BOTH `ai-assistant` objects from the `external` domain (the objects whose `path` ends in `ai-assistant/`, currently around lines 2914-2944). Then add a new top-level domain after `"external"`:

```json
    "ai": {
      "rust_module": "routes/ai.rs",
      "endpoints": [
        {
          "methods": ["POST"],
          "path": "/api/workspaces/:slug/projects/:project_id/ai-assistant/",
          "django_source": "app/urls/external.py:14",
          "rust_status": "stays_on_django",
          "rust_handler": "",
          "fe_evidence": [],
          "fe_pages": [],
          "batch_task": "Batch F0",
          "out_scope": false,
          "notes": "GPTIntegrationEndpoint.post (views/external/base.py:148-181): 400 LLM provider API key and model are required (base.py:153-157)/Task is required (base.py:159-161), 500 An internal error has occurred. (base.py:164-168), 200 {response,response_html,project_detail,workspace_detail} (base.py:173-181); ADMIN+MEMBER project gate (base.py:149); NO rust handler and NO main.rs route -> STAYS ON DJANGO per ADR F0-3; no FE caller found",
          "adr": "docs/superpowers/decisions/2026-09-10-f0-stays-on-django.md"
        },
        {
          "methods": ["POST"],
          "path": "/api/workspaces/:slug/ai-assistant/",
          "django_source": "app/urls/external.py:19",
          "rust_status": "deviation_accepted",
          "rust_handler": "routes::ai::workspace_ai_assistant",
          "fe_evidence": [
            {
              "service": "apps/web/core/services/ai.service.ts",
              "method": "createGptTask"
            }
          ],
          "fe_pages": [],
          "batch_task": "AI-1",
          "out_scope": false,
          "notes": "DECISION AI-1 (deviation_accepted, ADR 2026-09-21-ai-assistant-rust.md): WorkspaceGPTIntegrationEndpoint.post (views/external/base.py:184-212) parity status/body (400 config/task byte-exact, 500 generik, 200 {response,response_html}); deltas: OpenAI-compatible via LLM_BASE_URL env (LLM_PROVIDER + allowlist model dropped), upstream 429 -> 429 Rate limit exceeded for <host>, prompt hilang/non-string -> '', task non-string -> 400, body JSON invalid -> 400 axum. Gate WORKSPACE ADMIN/MEMBER verbatim (permissions/base.py:44-51,81-84). has_llm_configured kini DB-aware.",
          "adr": "docs/superpowers/decisions/2026-09-21-ai-assistant-rust.md"
        }
      ]
    },
```

- [ ] **Step 8: Run the parity gates**

Run: `cargo test -p api --test route_inventory_test --test fe_tripwire_test` (workdir `apps/api-rs`)

Expected: PASS. If `implemented_paths_are_registered_in_main_rs` fails, the route string in `main.rs` does not match the inventory path exactly — fix the route.

- [ ] **Step 9: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/ai.rs apps/api-rs/crates/api/src/main.rs apps/api-rs/crates/api/parity-inventory.json docs/superpowers/decisions/2026-09-21-ai-assistant-rust.md
git commit -m "feat(api-rs): workspace ai-assistant handler, route, and inventory decision"
```

---

### Task 4: DB-aware `has_llm_configured`

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/instance.rs:41-64,148`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)]` module in `apps/api-rs/crates/api/src/routes/instance.rs`:

```rust
    #[test]
    fn build_config_carries_llm_flag() {
        assert_eq!(build_config(true)["has_llm_configured"], json!(true));
        assert_eq!(build_config(false)["has_llm_configured"], json!(false));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p api --lib routes::instance` (workdir `apps/api-rs`)

Expected: compile error — `build_config` takes 0 arguments but 1 was supplied.

- [ ] **Step 3: Implement**

In `apps/api-rs/crates/api/src/routes/instance.rs`:

1. Add the import at the top:

```rust
use crate::routes::ai::resolve_llm_config;
```

2. Change the signature and the flag line:

```rust
pub fn build_config(has_llm_configured: bool) -> Value {
```

and

```rust
        "has_llm_configured": has_llm_configured,
```

3. In `get()`, before the final `Ok(...)`, resolve the config and pass the flag:

```rust
    let llm = resolve_llm_config(&st.pool).await;

    Ok((
        StatusCode::OK,
        Json(json!({"config": build_config(!llm.api_key.is_empty()), "instance": instance})),
    ))
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p api --lib routes::instance` (workdir `apps/api-rs`)

Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/instance.rs
git commit -m "fix(api-rs): has_llm_configured reads instance configurations like django"
```

---

### Task 5: Stale docs/comments + smoke check

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/search.rs:22`
- Modify: `docker-compose.yml:37-38`
- Modify: `docs/design/01-architecture.md:141`
- Modify: `docs/business-capabilities.md:16`
- Modify: `docs/superpowers/decisions/2026-09-10-f0-stays-on-django.md`
- Modify: `apps/api-rs/scripts/smoke.sh` (before `echo "== cleanup =="` at line 617)

- [ ] **Step 1: Add the smoke check**

In `apps/api-rs/scripts/smoke.sh`, insert before `echo "== cleanup =="`:

```bash
echo "== ai =="
# Route harus dilayani Rust (bukan 404/405). Status bergantung konfigurasi
# stack: 400 tanpa key, 200/500/429 bila key ada.
AICODE=$(curl "${H[@]}" -o /tmp/smoke_body -w '%{http_code}' -X POST -d '{"task":"say hi","prompt":"hi"}' "$BASE/api/workspaces/$WS/ai-assistant/")
if [ "$AICODE" = "404" ] || [ "$AICODE" = "405" ]; then
  FAIL=$((FAIL+1)); FAILED="$FAILED ai-assistant($AICODE)"; echo "FAIL ai-assistant route missing -> $AICODE"
else
  PASS=$((PASS+1)); echo "ok   ai-assistant served -> $AICODE"
fi
```

- [ ] **Step 2: Syntax-check the script**

Run: `bash -n apps/api-rs/scripts/smoke.sh`

Expected: no output (exit 0).

- [ ] **Step 3: Update the stale comments/docs**

`apps/api-rs/crates/api/src/routes/search.rs` — replace the `STAYS ON DJANGO` paragraph:

```rust
/// STAYS ON DJANGO (`plane/app/urls/external.py`): Unsplash and the
/// project-level GPT AI-assistant — third-party API proxies needing external
/// keys. The workspace-level `ai-assistant/` moved to `routes/ai.rs`
/// (ADR 2026-09-21).
```

`docker-compose.yml:37-38` — replace `Unsplash/GPT external` with `Unsplash external`.

`docs/design/01-architecture.md:141` — in the boundary list, replace
`asset S3 upload/download, Unsplash/GPT external, analytic export, notification sending, OAuth`
with
`asset S3 upload/download, Unsplash external, analytic export, notification sending, OAuth`.

`docs/business-capabilities.md:16` — replace `Boundary belum di-port: asset S3, external, export, notif, OAuth`
with `Boundary belum di-port: asset S3, external (Unsplash), export, notif, OAuth`.

`docs/superpowers/decisions/2026-09-10-f0-stays-on-django.md` — under the `## GPT proxy ×2` heading, append:

```markdown
> **Superseded (2026-09-21):** endpoint workspace
> `POST /api/workspaces/:slug/ai-assistant/` kini dibangun di Rust
> (`routes/ai.rs`, `deviation_accepted`) — lihat
> `docs/superpowers/decisions/2026-09-21-ai-assistant-rust.md`. Bagian
> project-level di bawah masih berlaku.
```

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/search.rs docker-compose.yml docs/design/01-architecture.md docs/business-capabilities.md docs/superpowers/decisions/2026-09-10-f0-stays-on-django.md apps/api-rs/scripts/smoke.sh
git commit -m "docs: ai assistant rust boundary and smoke coverage"
```

---

### Task 6: Full verification

**Files:** none (verification only)

- [ ] **Step 1: Unit + integration + gates**

Run (workdir `apps/api-rs`):

```bash
cargo test -p api --lib
cargo test -p api --test ai_test
cargo test -p api --test route_inventory_test
cargo test -p api --test fe_tripwire_test
```

Expected: all PASS, 0 failed.

- [ ] **Step 2: Full workspace test with test DB**

Run (workdir `apps/api-rs`, DB IP from `docs/design/05-testing-strategy.md:42`):

```bash
DATABASE_URL=postgres://plane:plane@<plane-db-ip>:5432/plane_test cargo test --workspace
```

Expected: 0 failed. `cutover_test` needs the stack up on :8000 — if it fails with `cutover api must listen`, start the stack or run only the non-live tests.

- [ ] **Step 3: Compile check the release build**

Run: `cargo build -p api` (workdir `apps/api-rs`)

Expected: success.

- [ ] **Step 4: Live smoke (stack up)**

Run:

```bash
TOKEN=<valid api_tokens.token> bash apps/api-rs/scripts/smoke.sh
```

Expected: `ok   ai-assistant served -> ...` and no regressions in the other checks.

- [ ] **Step 5: Manual UI check (optional, tunnel)**

Set `LLM_API_KEY` + `LLM_MODEL` via the admin AI form, set `LLM_BASE_URL` in `apps/api/.env` if not OpenAI, restart `api`. Open an issue → "I'm feeling lucky" button is visible (gate `has_llm_configured`) → response fills the description. Without a key, the button stays hidden.

---

## Notes for the implementer

- Rust formatting: run `cargo fmt` before committing if the repo CI checks it.
- Never log `api_key`; `chat_completion` logs only upstream status + a 500-char body snippet.
- The route string in `main.rs` must match the inventory path exactly: `/api/workspaces/:slug/ai-assistant/`.
- `deviation_accepted` gate rules (`route_inventory_test.rs:127-170`): `rust_handler` non-empty, `adr` file exists, `notes` contains `DECISION`.
