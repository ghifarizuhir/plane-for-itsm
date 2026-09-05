use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Sweep-up for the small URL modules:
/// - `GET /api/timezones/` (plane/app/urls/timezone.py:TimezoneEndpoint,
///   AllowAny): static label/value list, codegen'd from the Django source
///   into `timezones.json` — no auth, like Django.
/// - `POST/GET workspaces/:slug/export-issues/`
///   (plane/app/urls/exporter.py): provider must be csv/xlsx/json
///   ("Provider 'x' not found."); GET requires per_page+cursor
///   ("per_page and cursor are required"). The row is recorded in
///   `exporters`; file generation stays on the Django celery worker
///   (`issue_export_task`) until the export task is ported.
/// - `users/api-tokens/` GET/POST + `users/api-tokens/:pk/` GET/PATCH/DELETE
///   (plane/app/urls/api.py:ApiTokenEndpoint): label defaults to uuid hex,
///   token `plane_api_<hex>` visible only on create.
/// - `workspaces/:slug/stickies/` + `/:pk/` (plane/api/urls/sticky.py:
///   StickyViewSet list/create/retrieve/update/delete); name optional.
///
/// STAYS ON DJANGO: `schema/`, `swagger-ui/`, `redoc/` (drf-spectacular
/// build-time docs, not API contract).
pub const EXPORT_PROVIDERS: [&str; 3] = ["csv", "xlsx", "json"];

pub fn validate_export_provider(provider: Option<&str>) -> Result<(), String> {
    match provider {
        Some(p) if EXPORT_PROVIDERS.contains(&p) => Ok(()),
        Some(p) => Err(format!("Provider '{p}' not found.")),
        None => Err("Provider 'unknown' not found.".to_string()),
    }
}

/// Mirrors `label = request.data.get("label", str(uuid4().hex))`.
pub fn default_token_label(label: Option<String>) -> String {
    label.unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string())
}

pub async fn timezones() -> Json<Value> {
    // Generated from apps/api/plane/app/views/timezone/base.py.
    const DATA: &str = include_str!("../timezones.json");
    Json(serde_json::from_str(DATA).unwrap_or(Value::Array(vec![])))
}

/// Deprecated: AuthUser kini membawa UUID user langsung (`auth.0`).
/// Dipertahankan hingga Task 6 selesai — jangan hapus dulu.
#[allow(dead_code)]
pub(crate) async fn user_id(st: &AppState, auth: &AuthUser) -> Result<uuid::Uuid, (StatusCode, Json<Value>)> {
    let id: Option<uuid::Uuid> = sqlx::query_scalar("SELECT user_id FROM api_tokens WHERE token = $1")
        .bind(&auth.0)
        .fetch_optional(&st.pool)
        .await
        .map_err(|_| (StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid api key"}))))?;
    id.ok_or((StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid api key"}))))
}

// ---- exporter ----

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CreateExport {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub multiple: Option<bool>,
    #[serde(default)]
    pub project: Option<Vec<uuid::Uuid>>,
}

pub async fn create_export(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Json(body): Json<CreateExport>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_export_provider(body.provider.as_deref()).map_err(|e| anyhow::anyhow!(e))?;
    let user = auth.0;
    let projects = body.project.unwrap_or_default();
    sqlx::query(
        "INSERT INTO exporters (id, workspace_id, project, provider, \"type\", initiated_by_id, status, reason, key, token, created_at, updated_at) SELECT gen_random_uuid(), w.id, $1, $2, 'issue_exports', $3, 'queued', '', '', 'exp_' || replace(gen_random_uuid()::text, '-', ''), now(), now() FROM workspaces w WHERE w.slug = $4",
    )
    .bind(&projects)
    .bind(body.provider.as_deref())
    .bind(user)
    .bind(&slug)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(json!({"message": "Once the export is ready you will be able to download it"}))))
}

#[derive(Debug, Deserialize, Default)]
pub struct ExportHistoryQuery {
    #[serde(default)]
    pub per_page: Option<i64>,
    #[serde(default)]
    pub cursor: Option<String>,
}

pub async fn export_history(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    axum::extract::Query(q): axum::extract::Query<ExportHistoryQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if q.per_page.is_none() || q.cursor.is_none() {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "per_page and cursor are required"}))));
    }
    let rows = sqlx::query_as::<_, common::models::misc::ExporterHistory>(
        "SELECT e.id, e.provider FROM exporters e JOIN workspaces w ON w.id = e.workspace_id WHERE w.slug = $1 AND e.\"type\" = 'issue_exports' AND e.deleted_at IS NULL ORDER BY e.created_at DESC LIMIT $2",
    )
    .bind(&slug)
    .bind(q.per_page.unwrap_or(10))
    .fetch_all(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(json!(rows.into_iter().map(|e| json!({"id": e.id, "provider": e.provider})).collect::<Vec<_>>()))))
}

// ---- api tokens ----

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CreateApiToken {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub expired_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiTokenOut {
    pub id: uuid::Uuid,
    pub label: String,
}

pub async fn list_tokens(
    State(st): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<Value>>, common::errors::AppError> {
    let user = auth.0;
    let rows = sqlx::query_as::<_, common::models::misc::ApiToken>(
        "SELECT id, label FROM api_tokens WHERE user_id = $1 AND is_service = false AND deleted_at IS NULL ORDER BY created_at DESC",
    )
    .bind(user)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(rows.into_iter().map(|t| json!({"id": t.id, "label": t.label})).collect()))
}

pub async fn create_token(
    State(st): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateApiToken>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let user = auth.0;
    let label = default_token_label(body.label);
    // user_type mirrors Django: 1 for bot callers, else 0.
    let is_bot: (bool,) = sqlx::query_as("SELECT is_bot FROM users WHERE id = $1")
        .bind(user)
        .fetch_one(&st.pool)
        .await?;
    // Token visible only on create, like APITokenSerializer.
    let row: (uuid::Uuid, String, String) = sqlx::query_as(
        "INSERT INTO api_tokens (id, label, description, token, user_id, user_type, is_active, is_service, allowed_rate_limit, expired_at, created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, 'plane_api_' || replace(gen_random_uuid()::text, '-', ''), $3, $4, true, false, '60/min', $5, now(), now()) RETURNING id, label, token",
    )
    .bind(&label)
    .bind(body.description.clone().unwrap_or_default())
    .bind(user)
    .bind(if is_bot.0 { 1 } else { 0 })
    .bind(body.expired_at)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({"id": row.0, "label": row.1, "token": row.2}))))
}

pub async fn get_token(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(pk): axum::extract::Path<uuid::Uuid>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let user = auth.0;
    let row: Option<common::models::misc::ApiToken> = sqlx::query_as(
        "SELECT id, label FROM api_tokens WHERE id = $1 AND user_id = $2 AND is_service = false AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(user)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(t) => Ok((StatusCode::OK, Json(json!({"id": t.id, "label": t.label})))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Token not found"})))),
    }
}

pub async fn delete_token(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(pk): axum::extract::Path<uuid::Uuid>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let user = auth.0;
    sqlx::query("UPDATE api_tokens SET deleted_at = now() WHERE id = $1 AND user_id = $2 AND is_service = false")
        .bind(pk)
        .bind(user)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

// ---- stickies ----

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CreateSticky {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

pub async fn list_stickies(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<Json<Vec<Value>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::misc::Sticky>(
        "SELECT s.id, s.name FROM stickies s JOIN workspaces w ON w.id = s.workspace_id WHERE w.slug = $1 AND s.deleted_at IS NULL ORDER BY s.created_at DESC",
    )
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(rows.into_iter().map(|s| json!({"id": s.id, "name": s.name})).collect()))
}

pub async fn create_sticky(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Json(body): Json<CreateSticky>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let user = auth.0;
    let row = sqlx::query_as::<_, common::models::misc::Sticky>(
        "INSERT INTO stickies (id, name, description, description_html, logo_props, color, sort_order, workspace_id, owner_id, created_at, updated_at) SELECT gen_random_uuid(), $1, '{}', '<p></p>', '{}', $2, 65535, w.id, $3, now(), now() FROM workspaces w WHERE w.slug = $4 RETURNING id, name",
    )
    .bind(&body.name)
    .bind(&body.color)
    .bind(user)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({"id": row.id, "name": row.name}))))
}

pub async fn get_sticky(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<common::models::misc::Sticky> = sqlx::query_as(
        "SELECT s.id, s.name FROM stickies s JOIN workspaces w ON w.id = s.workspace_id WHERE w.slug = $1 AND s.id = $2 AND s.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(s) => Ok((StatusCode::OK, Json(json!({"id": s.id, "name": s.name})))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Sticky not found"})))),
    }
}

pub async fn patch_sticky(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateSticky>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let n = sqlx::query(
        "UPDATE stickies s SET name = COALESCE($1, name), color = COALESCE($2, color), updated_at = now() FROM workspaces w WHERE w.id = s.workspace_id AND w.slug = $3 AND s.id = $4 AND s.deleted_at IS NULL",
    )
    .bind(&body.name)
    .bind(&body.color)
    .bind(&slug)
    .bind(pk)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Sticky not found"}))));
    }
    Ok((StatusCode::OK, Json(json!({"id": pk}))))
}

pub async fn delete_sticky(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    sqlx::query(
        "UPDATE stickies s SET deleted_at = now() FROM workspaces w WHERE w.id = s.workspace_id AND w.slug = $1 AND s.id = $2 AND s.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(pk)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}
