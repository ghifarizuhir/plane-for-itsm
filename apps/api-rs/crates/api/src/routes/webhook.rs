use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

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

#[derive(Debug, Clone, Serialize)]
pub struct WebhookOut {
    pub id: uuid::Uuid,
    pub url: String,
    pub is_active: bool,
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

pub async fn list(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<Json<Vec<WebhookOut>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::webhook::Webhook>(
        "SELECT wh.id, wh.url, wh.is_active FROM webhooks wh JOIN workspaces w ON w.id = wh.workspace_id WHERE w.slug = $1 AND wh.deleted_at IS NULL ORDER BY wh.created_at DESC",
    )
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|wh| WebhookOut { id: wh.id, url: wh.url, is_active: wh.is_active })
            .collect(),
    ))
}

pub async fn create(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Json(body): Json<CreateWebhook>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;

    let existing: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT wh.id FROM webhooks wh JOIN workspaces w ON w.id = wh.workspace_id WHERE w.slug = $1 AND wh.url = $2 AND wh.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(&body.url)
    .fetch_optional(&st.pool)
    .await?;
    if existing.is_some() {
        return Ok((StatusCode::CONFLICT, Json(json!({"error": "URL already exists for the workspace"}))));
    }

    let row = sqlx::query_as::<_, common::models::webhook::Webhook>(
        "INSERT INTO webhooks (id, url, is_active, secret_key, project, issue, module, cycle, issue_comment, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, 'plane_wh_' || replace(gen_random_uuid()::text, '-', ''), $3, $4, $5, $6, $7, w.id, now(), now() FROM workspaces w WHERE w.slug = $8 RETURNING id, url, is_active",
    )
    .bind(&body.url)
    .bind(body.is_active.unwrap_or(true))
    .bind(body.project.unwrap_or(false))
    .bind(body.issue.unwrap_or(false))
    .bind(body.module.unwrap_or(false))
    .bind(body.cycle.unwrap_or(false))
    .bind(body.issue_comment.unwrap_or(false))
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({"id": row.id, "url": row.url, "is_active": row.is_active}))))
}

pub async fn regenerate(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<(uuid::Uuid, String)> = sqlx::query_as(
        "UPDATE webhooks wh SET secret_key = 'plane_wh_' || replace(gen_random_uuid()::text, '-', '') FROM workspaces w WHERE w.id = wh.workspace_id AND w.slug = $1 AND wh.id = $2 AND wh.deleted_at IS NULL RETURNING wh.id, wh.secret_key",
    )
    .bind(&slug)
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some((id, secret)) => Ok((StatusCode::OK, Json(json!({"id": id, "secret_key": secret})))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Webhook not found"})))),
    }
}

pub async fn list_logs(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, webhook_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<Value>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::webhook::WebhookLog>(
        "SELECT wl.id, wl.event_type FROM webhook_logs wl JOIN workspaces w ON w.id = wl.workspace_id WHERE w.slug = $1 AND wl.webhook = $2 ORDER BY wl.created_at DESC",
    )
    .bind(&slug)
    .bind(webhook_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|l| json!({"id": l.id, "event_type": l.event_type}))
            .collect(),
    ))
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchWebhook {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub is_active: Option<bool>,
}

/// Mirrors `plane/app/views/webhook/base.py:WebhookEndpoint` get / patch /
/// delete on the workspace webhook detail route.
pub async fn detail(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<common::models::webhook::Webhook> = sqlx::query_as(
        "SELECT wh.id, wh.url, wh.is_active FROM webhooks wh JOIN workspaces w ON w.id = wh.workspace_id WHERE wh.id = $1 AND w.slug = $2 AND wh.deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(wh) => Ok((
            StatusCode::OK,
            Json(json!({"id": wh.id, "url": wh.url, "is_active": wh.is_active})),
        )),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Webhook not found"})))),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<PatchWebhook>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if let Some(url) = &body.url {
        validate_create(&CreateWebhook { url: url.clone(), is_active: body.is_active, project: None, issue: None, cycle: None, module: None, issue_comment: None })
            .map_err(|e| anyhow::anyhow!(e))?;
    }
    let n = sqlx::query(
        "UPDATE webhooks wh SET url = COALESCE($1, wh.url), is_active = COALESCE($2, wh.is_active), updated_at = now() FROM workspaces w WHERE w.id = wh.workspace_id AND w.slug = $3 AND wh.id = $4 AND wh.deleted_at IS NULL",
    )
    .bind(&body.url)
    .bind(body.is_active)
    .bind(&slug)
    .bind(pk)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Webhook not found"}))));
    }
    Ok((StatusCode::OK, Json(json!({"id": pk}))))
}

pub async fn destroy(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    sqlx::query(
        "UPDATE webhooks wh SET deleted_at = now() FROM workspaces w WHERE w.id = wh.workspace_id AND w.slug = $1 AND wh.id = $2 AND wh.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(pk)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}
