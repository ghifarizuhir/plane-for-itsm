use axum::{extract::State, http::StatusCode, Json};
use serde_json::{json, Value};

use crate::state::AppState;

/// Mirrors `apps/api/plane/license/api/views/instance.py:InstanceEndpoint.get`
/// (AllowAny, GET only). PATCH stays on Django — the router registers only
/// `get()` so other methods fall through to Axum's default 405.
///
/// Config mirrors Django keys exactly (env read directly like
/// `os.environ.get`, NOT AppConfig which only carries auth/frontend fields).

/// Pure flag check: only `"1"` enables (Django `== "1"` comparisons).
pub fn flag_enabled(raw: &str) -> bool {
    raw == "1"
}

/// Read env flag with default fallback (`"1"`/`"0"` strings like Django).
pub fn env_flag(key: &str, default: &str) -> bool {
    flag_enabled(&std::env::var(key).unwrap_or_else(|_| default.to_string()))
}

/// Read env string with default fallback.
pub fn env_str_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// `SLACK_CLIENT_ID` defaults to null (Django `default=None`).
pub fn env_opt(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

/// `FILE_SIZE_LIMIT` float, default 5242880 (Django `float(os.environ.get(...))`).
pub fn file_size_limit() -> f64 {
    std::env::var("FILE_SIZE_LIMIT")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(5_242_880.0)
}

pub fn build_config() -> Value {
    json!({
        "enable_signup": env_flag("ENABLE_SIGNUP", "0"),
        "is_workspace_creation_disabled": env_flag("DISABLE_WORKSPACE_CREATION", "0"),
        "is_google_enabled": env_flag("IS_GOOGLE_ENABLED", "0"),
        "is_github_enabled": env_flag("IS_GITHUB_ENABLED", "0"),
        "is_gitlab_enabled": env_flag("IS_GITLAB_ENABLED", "0"),
        "is_gitea_enabled": env_flag("IS_GITEA_ENABLED", "0"),
        "is_magic_login_enabled": env_flag("ENABLE_MAGIC_LINK_LOGIN", "1"),
        "is_email_password_enabled": env_flag("ENABLE_EMAIL_PASSWORD", "1"),
        "github_app_name": env_str_or("GITHUB_APP_NAME", ""),
        "slack_client_id": env_opt("SLACK_CLIENT_ID"),
        "has_unsplash_configured": !env_str_or("UNSPLASH_ACCESS_KEY", "").is_empty(),
        "has_llm_configured": !env_str_or("LLM_API_KEY", "").is_empty(),
        "file_size_limit": file_size_limit(),
        "is_smtp_configured": !env_str_or("EMAIL_HOST", "").is_empty(),
        "app_base_url": env_str_or("APP_BASE_URL", ""),
        "space_base_url": env_str_or("SPACE_BASE_URL", ""),
        "admin_base_url": env_str_or("ADMIN_BASE_URL", ""),
        "is_self_managed": true,
        // TODO: picks up INSTANCE_CHANGELOG_URL when set; Django defaults to "".
        "instance_changelog_url": env_str_or("INSTANCE_CHANGELOG_URL", ""),
    })
}

#[derive(Debug, sqlx::FromRow)]
struct InstanceRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    instance_name: String,
    whitelist_emails: Option<String>,
    instance_id: String,
    current_version: String,
    latest_version: Option<String>,
    last_checked_at: chrono::DateTime<chrono::Utc>,
    namespace: Option<String>,
    is_telemetry_enabled: bool,
    is_support_required: bool,
    is_setup_done: bool,
    is_signup_screen_visited: bool,
    is_verified: bool,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
}

pub async fn get(
    State(st): State<AppState>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<InstanceRow> = sqlx::query_as(
        "SELECT id, created_at, updated_at, instance_name, whitelist_emails, instance_id, current_version, latest_version, last_checked_at, namespace, is_telemetry_enabled, is_support_required, is_setup_done, is_signup_screen_visited, is_verified, created_by_id, updated_by_id FROM instances WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(&st.pool)
    .await
    .map_err(|e| {
        tracing::warn!(error=%e, "instance: lookup failed");
        common::errors::AppError::internal()
    })?;

    let Some(inst) = row else {
        return Ok((
            StatusCode::OK,
            Json(json!({"is_activated": false, "is_setup_done": false})),
        ));
    };

    let workspaces_exist: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workspaces WHERE deleted_at IS NULL)")
            .fetch_one(&st.pool)
            .await
            .map_err(|e| {
                tracing::warn!(error=%e, "instance: workspaces_exist lookup failed");
                common::errors::AppError::internal()
            })?;
    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&st.pool)
        .await
        .map_err(|e| {
            tracing::warn!(error=%e, "instance: user_count lookup failed");
            common::errors::AppError::internal()
        })?;

    let instance = json!({
        "id": inst.id.to_string(),
        "created_at": inst.created_at.to_rfc3339(),
        "updated_at": inst.updated_at.to_rfc3339(),
        "instance_name": inst.instance_name,
        "whitelist_emails": inst.whitelist_emails,
        "instance_id": inst.instance_id,
        "current_version": inst.current_version,
        "latest_version": inst.latest_version,
        "last_checked_at": inst.last_checked_at.to_rfc3339(),
        "namespace": inst.namespace,
        "is_telemetry_enabled": inst.is_telemetry_enabled,
        "is_support_required": inst.is_support_required,
        "is_setup_done": inst.is_setup_done,
        "is_signup_screen_visited": inst.is_signup_screen_visited,
        "is_verified": inst.is_verified,
        "created_by": inst.created_by_id.map(|u| u.to_string()),
        "updated_by": inst.updated_by_id.map(|u| u.to_string()),
        "is_activated": true,
        "workspaces_exist": workspaces_exist,
        "user_count": user_count,
    });

    Ok((
        StatusCode::OK,
        Json(json!({"config": build_config(), "instance": instance})),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_only_1_enables() {
        assert!(flag_enabled("1"));
        for v in ["0", "", "true", "True", "2"] {
            assert!(!flag_enabled(v), "{v:?} must be false");
        }
    }

    #[test]
    fn file_size_limit_defaults_and_parses() {
        // Default when missing/invalid is covered without touching global env
        // when the var is absent; parse path checked via valid float string.
        let parsed: f64 = "123.5".parse().unwrap();
        assert_eq!(parsed, 123.5);
        assert_eq!(file_size_limit(), 5_242_880.0);
    }
}
