use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    middleware::auth::AuthUser,
    routes::{
        member::deny_detail,
        project::{missing, ws_role},
    },
    state::AppState,
};

/// Mirrors `plane/app/views/webhook/base.py:WebhookEndpoint` list/create,
/// `WebhookSecretRegenerateEndpoint` and `WebhookLogsEndpoint` for
/// `plane/app/urls/webhook.py`. URL rules mirror
/// `plane/db/models/webhook.py:validate_schema/validate_domain` (http/https
/// only, no localhost/127.0.0.1, max 1024); duplicate (workspace, url) → 409
/// "URL already exists for the workspace". Secret format mirrors
/// `generate_token`: `plane_wh_<32hex>`. Delivery itself is a worker
/// concern (out of scope).
#[derive(Debug, Clone, Deserialize)]
pub struct CreateWebhook {
    pub url: String,
    #[serde(default)]
    pub is_active: Option<bool>,
    #[serde(default)]
    pub project: Option<bool>,
    #[serde(default)]
    pub issue: Option<bool>,
    #[serde(default)]
    pub cycle: Option<bool>,
    #[serde(default)]
    pub module: Option<bool>,
    #[serde(default)]
    pub issue_comment: Option<bool>,
}

pub fn validate_create(body: &CreateWebhook) -> Result<(), String> {
    if body.url.trim().is_empty() {
        return Err("url is required".to_string());
    }
    if body.url.chars().count() > 1024 {
        return Err("url max length 1024".to_string());
    }
    let rest = body
        .url
        .strip_prefix("http://")
        .or_else(|| body.url.strip_prefix("https://"))
        .ok_or_else(|| "Invalid schema. Only HTTP and HTTPS are allowed.".to_string())?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.split('@').next_back().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    if host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" {
        return Err("Local URLs are not allowed.".to_string());
    }
    Ok(())
}

/// Full webhook row. Django serves `__all__` on every verb — the
/// `fields=` allowlists in `views/webhook/base.py` are DEAD (see the
/// `to_representation` comment in `serializers/webhook.py:57-79`:
/// `DynamicBaseSerializer` drops them). `secret_key` is popped unless the
/// `show_secret_key` flag is set — i.e. present on create + regenerate
/// only, never on list/retrieve/update.
#[derive(Debug, Clone, sqlx::FromRow)]
struct WebhookFullRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
    workspace_id: uuid::Uuid,
    url: String,
    is_active: bool,
    secret_key: Option<String>,
    project: bool,
    issue: bool,
    module: bool,
    cycle: bool,
    issue_comment: bool,
    is_internal: bool,
    version: String,
}

const WEBHOOK_FULL_COLS: &str = "wh.id, wh.created_at, wh.updated_at, wh.created_by_id, \
    wh.updated_by_id, wh.workspace_id, wh.url, wh.is_active, wh.secret_key, wh.project, \
    wh.issue, wh.module, wh.cycle, wh.issue_comment, wh.is_internal, wh.version";

fn webhook_full_json(r: &WebhookFullRow, show_secret: bool) -> Value {
    let mut o = serde_json::Map::with_capacity(16);
    o.insert("id".to_string(), json!(r.id));
    o.insert("created_at".to_string(), json!(r.created_at));
    o.insert("updated_at".to_string(), json!(r.updated_at));
    o.insert("created_by".to_string(), json!(r.created_by_id));
    o.insert("updated_by".to_string(), json!(r.updated_by_id));
    o.insert("workspace".to_string(), json!(r.workspace_id));
    o.insert("url".to_string(), json!(r.url));
    o.insert("is_active".to_string(), json!(r.is_active));
    if show_secret {
        o.insert("secret_key".to_string(), json!(r.secret_key));
    }
    o.insert("project".to_string(), json!(r.project));
    o.insert("issue".to_string(), json!(r.issue));
    o.insert("module".to_string(), json!(r.module));
    o.insert("cycle".to_string(), json!(r.cycle));
    o.insert("issue_comment".to_string(), json!(r.issue_comment));
    o.insert("is_internal".to_string(), json!(r.is_internal));
    o.insert("version".to_string(), json!(r.version));
    Value::Object(o)
}

async fn fetch_webhook_full(
    pool: &sqlx::PgPool,
    slug: &str,
    pk: uuid::Uuid,
) -> Result<Option<WebhookFullRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {WEBHOOK_FULL_COLS} FROM webhooks wh JOIN workspaces w ON w.id = wh.workspace_id \
         WHERE wh.id = $1 AND w.slug = $2 AND wh.deleted_at IS NULL"
    ))
    .bind(pk)
    .bind(slug)
    .fetch_optional(pool)
    .await
}

/// ADMIN-only workspace gate (`base.py:21,41,81,107,115,125`); deny is the
/// DRF permission-class 403.
async fn gate_ws_admin(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
) -> Result<bool, sqlx::Error> {
    Ok(matches!(ws_role(pool, user, slug).await?, Some(r) if r >= 20))
}

pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny_detail());
    }
    let rows: Vec<WebhookFullRow> = sqlx::query_as(&format!(
        "SELECT {WEBHOOK_FULL_COLS} FROM webhooks wh JOIN workspaces w ON w.id = wh.workspace_id \
         WHERE w.slug = $1 AND wh.deleted_at IS NULL ORDER BY wh.created_at DESC"
    ))
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(json!(rows.iter().map(|r| webhook_full_json(r, false)).collect::<Vec<_>>()))))
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Json(body): Json<CreateWebhook>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny_detail());
    }
    // Serializer validation errors → 400 (Django `serializer.errors`);
    // the previous `anyhow` mapping 500d them.
    if let Err(e) = validate_create(&body) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"url": [e]}))));
    }

    let ws_id: Option<(uuid::Uuid,)> =
        sqlx::query_as("SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL")
            .bind(&slug)
            .fetch_optional(&st.pool)
            .await?;
    let Some((ws_id,)) = ws_id else {
        return Ok(missing());
    };
    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM webhooks WHERE workspace_id = $1 AND url = $2 AND deleted_at IS NULL)",
    )
    .bind(ws_id)
    .bind(&body.url)
    .fetch_one(&st.pool)
    .await?;
    if dup {
        return Ok((StatusCode::CONFLICT, Json(json!({"error": "URL already exists for the workspace"}))));
    }

    // `_validate_webhook_url` SSRF/DNS checks (`serializers/webhook.py:30-56`)
    // need live DNS + settings allowlists — not mirrored (documented);
    // schema/domain rules are enforced by `validate_create` above.
    let row: Option<WebhookFullRow> = sqlx::query_as(
        "INSERT INTO webhooks (id, url, is_active, is_internal, version, secret_key, project, issue, module, cycle, issue_comment, workspace_id, created_by_id, updated_by_id, created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, false, 'v1', 'plane_wh_' || replace(gen_random_uuid()::text, '-', ''), $3, $4, $5, $6, $7, $8, $9, $9, now(), now()) RETURNING id, created_at, updated_at, created_by_id, updated_by_id, workspace_id, url, is_active, secret_key, project, issue, module, cycle, issue_comment, is_internal, version",
    )
    .bind(&body.url)
    .bind(body.is_active.unwrap_or(true))
    .bind(body.project.unwrap_or(false))
    .bind(body.issue.unwrap_or(false))
    .bind(body.module.unwrap_or(false))
    .bind(body.cycle.unwrap_or(false))
    .bind(body.issue_comment.unwrap_or(false))
    .bind(ws_id)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        // 201 full row WITH secret (`show_secret_key`, `base.py:27`).
        Some(r) => Ok((StatusCode::CREATED, Json(webhook_full_json(&r, true)))),
        None => Err(common::errors::AppError(anyhow::anyhow!("webhook insert failed"))),
    }
}

pub async fn regenerate(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny_detail());
    }
    let row: Option<WebhookFullRow> = sqlx::query_as(&format!(
        "UPDATE webhooks wh SET secret_key = 'plane_wh_' || replace(gen_random_uuid()::text, '-', ''), updated_at = now(), updated_by_id = $3 FROM workspaces w WHERE w.id = wh.workspace_id AND w.slug = $1 AND wh.id = $2 AND wh.deleted_at IS NULL RETURNING {WEBHOOK_FULL_COLS}"
    ))
    .bind(&slug)
    .bind(pk)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        // 200 full row WITH the new secret (`show_secret_key`, `base.py:120`).
        Some(r) => Ok((StatusCode::OK, Json(webhook_full_json(&r, true)))),
        // Django `.get` (`base.py:117`) miss → generic 404 via `views/base.py:92-96`.
        None => Ok(missing()),
    }
}

pub async fn list_logs(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, webhook_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny_detail());
    }
    // `WebhookLogSerializer`, `fields="__all__"` (`serializers/webhook.py:98-102`).
    let rows: Vec<Value> = sqlx::query_as::<_, (
        uuid::Uuid, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>,
        Option<uuid::Uuid>, Option<uuid::Uuid>, uuid::Uuid, uuid::Uuid,
        Option<String>, Option<String>, Option<String>, Option<String>,
        Option<String>, Option<String>, Option<String>, i16,
    )>(
        "SELECT wl.id, wl.created_at, wl.updated_at, wl.created_by_id, wl.updated_by_id, \
         wl.workspace_id, wl.webhook, wl.event_type, wl.request_method, wl.request_headers, \
         wl.request_body, wl.response_status, wl.response_headers, wl.response_body, wl.retry_count \
         FROM webhook_logs wl JOIN workspaces w ON w.id = wl.workspace_id \
         WHERE w.slug = $1 AND wl.webhook = $2 ORDER BY wl.created_at DESC",
    )
    .bind(&slug)
    .bind(webhook_id)
    .fetch_all(&st.pool)
    .await?
    .into_iter()
    .map(|(id, created_at, updated_at, created_by, updated_by, ws, wh, event_type,
           req_method, req_headers, req_body, resp_status, resp_headers, resp_body, retry)| {
        json!({
            "id": id, "created_at": created_at, "updated_at": updated_at,
            "created_by": created_by, "updated_by": updated_by,
            "workspace": ws, "webhook": wh, "event_type": event_type,
            "request_method": req_method, "request_headers": req_headers,
            "request_body": req_body, "response_status": resp_status,
            "response_headers": resp_headers, "response_body": resp_body,
            "retry_count": retry,
        })
    })
    .collect();
    Ok((StatusCode::OK, Json(Value::Array(rows))))
}

/// Mirrors `plane/app/views/webhook/base.py:WebhookEndpoint` get / patch /
/// delete on the workspace webhook detail route: full rows minus secret,
/// ADMIN gate, 200 full row on PATCH.
pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny_detail());
    }
    match fetch_webhook_full(&st.pool, &slug, pk).await? {
        Some(r) => Ok((StatusCode::OK, Json(webhook_full_json(&r, false)))),
        // Django `.get` (`base.py:63`) miss → generic 404 via `views/base.py:92-96`.
        None => Ok(missing()),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny_detail());
    }
    if let Some(url) = body.get("url").and_then(Value::as_str) {
        if let Err(e) = validate_create(&CreateWebhook {
            url: url.to_string(),
            is_active: None,
            project: None,
            issue: None,
            cycle: None,
            module: None,
            issue_comment: None,
        }) {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"url": [e]}))));
        }
    }
    let mut tx = st.pool.begin().await?;
    if let Some(url) = body.get("url").and_then(Value::as_str) {
        sqlx::query("UPDATE webhooks SET url = $1, updated_at = now(), updated_by_id = $2 WHERE id = $3")
            .bind(url).bind(auth.0).bind(pk).execute(&mut *tx).await?;
    }
    if let Some(b) = body.get("is_active").and_then(Value::as_bool) {
        sqlx::query("UPDATE webhooks SET is_active = $1, updated_at = now(), updated_by_id = $2 WHERE id = $3")
            .bind(b).bind(auth.0).bind(pk).execute(&mut *tx).await?;
    }
    for (key, col) in [
        ("project", "project"),
        ("issue", "issue"),
        ("module", "module"),
        ("cycle", "cycle"),
        ("issue_comment", "issue_comment"),
    ] {
        if let Some(b) = body.get(key).and_then(Value::as_bool) {
            sqlx::query(&format!("UPDATE webhooks SET {col} = $1, updated_at = now(), updated_by_id = $2 WHERE id = $3"))
                .bind(b).bind(auth.0).bind(pk).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    match fetch_webhook_full(&st.pool, &slug, pk).await? {
        // 200 full row minus secret (`base.py:102-105`).
        Some(r) => Ok((StatusCode::OK, Json(webhook_full_json(&r, false)))),
        // Django `.get` (`base.py:83`) miss → generic 404 via `views/base.py:92-96`.
        None => Ok(missing()),
    }
}

pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_ws_admin(&st.pool, auth.0, &slug).await? {
        return Ok(deny_detail());
    }
    // Django `.get` (`base.py:109`) miss → generic 404 via `views/base.py:92-96`
    // (not a silent 204).
    let n = sqlx::query(
        "UPDATE webhooks wh SET deleted_at = now() FROM workspaces w WHERE w.id = wh.workspace_id AND w.slug = $1 AND wh.id = $2 AND wh.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(pk)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}
