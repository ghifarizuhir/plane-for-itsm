use axum::{extract::Query, extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};
use crate::routes::issue_common::{
    next_cursor_str, page_window, parse_cursor, parse_per_page, prev_cursor_str,
    total_pages, DetailEnvelope, PageWindow,
};
use crate::routes::member::deny_detail;
use crate::routes::page::{clean_description_html, decode_description_binary, strip_tags_text};
use crate::routes::project::{deny, missing, ws_role};

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

/// Full `StickySerializer` row (`serializers/workspace.py:339-344`,
/// `fields="__all__"`, FKs as PKs). `description_binary` rides base64 on the
/// wire — the same convention as pages (`page.py:180-198`); FE `TSticky`
/// never reads it.
#[derive(Debug, Clone, sqlx::FromRow)]
struct StickyRow {
    id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    workspace_id: uuid::Uuid,
    owner_id: uuid::Uuid,
    name: Option<String>,
    description: Value,
    description_html: String,
    description_stripped: Option<String>,
    description_binary: Option<Vec<u8>>,
    logo_props: Value,
    color: Option<String>,
    background_color: Option<String>,
    sort_order: f64,
    created_by_id: Option<uuid::Uuid>,
    updated_by_id: Option<uuid::Uuid>,
}

const STICKY_COLS: &str = "s.id, s.created_at, s.updated_at, s.workspace_id, s.owner_id, \
    s.name, s.description, s.description_html, s.description_stripped, s.description_binary, \
    s.logo_props, s.color, s.background_color, s.sort_order, s.created_by_id, s.updated_by_id";

fn sticky_json(r: &StickyRow) -> Value {
    use base64::Engine as _;
    json!({
        "id": r.id,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "workspace": r.workspace_id,
        "owner": r.owner_id,
        "name": r.name,
        "description": r.description,
        "description_html": r.description_html,
        "description_stripped": r.description_stripped,
        "description_binary": r.description_binary.as_ref().map(|b| base64::engine::general_purpose::STANDARD.encode(b)),
        "logo_props": r.logo_props,
        "color": r.color,
        "background_color": r.background_color,
        "sort_order": r.sort_order,
        "created_by": r.created_by_id,
        "updated_by": r.updated_by_id,
    })
}

#[derive(Debug, Deserialize, Default)]
pub struct StickyListQuery {
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    per_page: Option<String>,
}

/// Mirrors `WorkspaceStickyViewSet.list` (`sticky.py:40-52`): AMG gate,
/// OWNER-scoped queryset (`get_queryset`, `sticky.py:22-30`), `-sort_order`
/// order, optional `description_stripped__icontains` filter, and the
/// `OffsetPaginator` envelope (`default_per_page=20`).
pub async fn list_stickies(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Query(q): Query<StickyListQuery>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // AMG at WORKSPACE level (`sticky.py:40`); DRF permission-class deny shape.
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny_detail());
    }
    let limit = match parse_per_page(q.per_page.as_deref()) {
        Ok(v) => v.min(20),
        Err(e) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": e}))));
        }
    };
    let cursor_raw = q.cursor.unwrap_or_else(|| format!("{limit}:0:0"));
    let cursor = match parse_cursor(&cursor_raw) {
        Ok(c) => c,
        Err(e) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": e}))));
        }
    };
    let window = match page_window(cursor.page, limit) {
        Ok(w) => w,
        Err(()) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"detail": "Error in parsing"}))));
        }
    };
    let search = q.query.unwrap_or_default();
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM stickies s JOIN workspaces w ON w.id = s.workspace_id \
         WHERE w.slug = $1 AND s.owner_id = $2 AND s.deleted_at IS NULL \
         AND ($3 = '' OR s.description_stripped ILIKE '%' || $3 || '%')",
    )
    .bind(&slug)
    .bind(auth.0)
    .bind(&search)
    .fetch_one(&st.pool)
    .await?;
    let mut rows: Vec<StickyRow> = match window {
        PageWindow::Rows(offset) => sqlx::query_as(&format!(
            "SELECT {STICKY_COLS} FROM stickies s JOIN workspaces w ON w.id = s.workspace_id \
             WHERE w.slug = $1 AND s.owner_id = $2 AND s.deleted_at IS NULL \
             AND ($3 = '' OR s.description_stripped ILIKE '%' || $3 || '%') \
             ORDER BY s.sort_order DESC, s.created_at DESC LIMIT $4 OFFSET $5",
        ))
        .bind(&slug)
        .bind(auth.0)
        .bind(&search)
        .bind(limit + 1)
        .bind(offset)
        .fetch_all(&st.pool)
        .await?,
        PageWindow::BeyondEnd => Vec::new(),
    };
    let next_page_results = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    let envelope = DetailEnvelope {
        grouped_by: None,
        sub_grouped_by: None,
        total_count: total,
        next_cursor: next_cursor_str(limit, cursor.page),
        prev_cursor: prev_cursor_str(limit, cursor.page),
        next_page_results,
        prev_page_results: cursor.page > 0,
        count: rows.len() as i64,
        total_pages: total_pages(total, limit),
        total_results: total,
        extra_stats: None,
        results: rows.iter().map(sticky_json).collect(),
    };
    Ok((StatusCode::OK, Json(json!(envelope))))
}

/// Validates the sticky write body (`StickySerializer.validate`,
/// `serializers/workspace.py:346-361`): bad html → `{"error": "html content
/// is not valid"}`; bad binary → `{"description_binary": [...]}` (page
/// wire rules). Returns the cleaned (html, binary) pair.
fn clean_sticky_body(body: &Value) -> Result<(String, Option<Vec<u8>>), (StatusCode, Json<Value>)> {
    let raw_html = body.get("description_html").and_then(Value::as_str).unwrap_or("<p></p>");
    let html = clean_description_html(raw_html)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "html content is not valid"}))))?;
    let binary: Option<Vec<u8>> = match body.get("description_binary") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => match decode_description_binary(s) {
            Ok(b) if b.is_empty() && s.is_empty() => None,
            Ok(b) => Some(b),
            Err(e) => {
                return Err((StatusCode::BAD_REQUEST, Json(json!({"description_binary": [e]}))));
            }
        },
        Some(_) => {
            return Err((StatusCode::BAD_REQUEST, Json(json!({"description_binary": ["Invalid binary data"]}))));
        }
    };
    Ok((html, binary))
}

/// Mirrors `WorkspaceStickyViewSet.create` (`sticky.py:31-38`): AMG gate,
/// 201 full row; `sort_order = max+10000` and `description_stripped` per
/// `Sticky.save` (`db/models/sticky.py:38-54`).
pub async fn create_sticky(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny_detail());
    }
    let (html, binary) = match clean_sticky_body(&body) {
        Ok(v) => v,
        Err(e) => return Ok(e),
    };
    let stripped = strip_tags_text(&html);
    let row: Option<StickyRow> = sqlx::query_as(&format!(
        "INSERT INTO stickies (id, name, description, description_html, description_stripped, \
         description_binary, logo_props, color, background_color, sort_order, workspace_id, \
         owner_id, created_by_id, updated_by_id, created_at, updated_at) \
         SELECT gen_random_uuid(), $1, COALESCE($2, '{{}}'), $3, $4, $5, COALESCE($6, '{{}}'), \
         $7, $8, COALESCE((SELECT MAX(s.sort_order) FROM stickies s \
           JOIN workspaces w2 ON w2.id = s.workspace_id WHERE w2.slug = $9 \
           AND s.deleted_at IS NULL), 65535) + 10000, \
         w.id, $10, $10, $10, now(), now() FROM workspaces w WHERE w.slug = $9 \
         RETURNING {STICKY_COLS}",
    ))
    .bind(body.get("name").and_then(Value::as_str))
    .bind(body.get("description").cloned().unwrap_or(json!({})))
    .bind(&html)
    .bind(if stripped.is_empty() { None } else { Some(stripped) })
    .bind(&binary)
    .bind(body.get("logo_props").cloned())
    .bind(body.get("color").and_then(Value::as_str))
    .bind(body.get("background_color").and_then(Value::as_str))
    .bind(&slug)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(r) => Ok((StatusCode::CREATED, Json(sticky_json(&r)))),
        None => Ok(missing()),
    }
}

/// Creator-scoped sticky fetch: owner rows are invisible to everyone else
/// (queryset filter), and a non-creator member gets the creator-gate 403
/// (`permissions/base.py:24-44`), NOT a 404.
async fn fetch_owned_sticky(
    pool: &sqlx::PgPool,
    slug: &str,
    pk: uuid::Uuid,
    user: uuid::Uuid,
) -> Result<Option<StickyRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {STICKY_COLS} FROM stickies s JOIN workspaces w ON w.id = s.workspace_id \
         WHERE w.slug = $1 AND s.id = $2 AND s.deleted_at IS NULL",
    ))
    .bind(slug)
    .bind(pk)
    .fetch_optional(pool)
    .await
    .map(|row: Option<StickyRow>| {
        row.filter(|r| r.owner_id == user)
    })
}

pub async fn get_sticky(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny());
    }
    match fetch_owned_sticky(&st.pool, &slug, pk, auth.0).await? {
        Some(r) => Ok((StatusCode::OK, Json(sticky_json(&r)))),
        None => {
            // Distinguish miss from non-creator: the row exists but is owned
            // by someone else → creator-gate 403; else the generic 404.
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM stickies s JOIN workspaces w ON w.id = s.workspace_id \
                 WHERE w.slug = $1 AND s.id = $2 AND s.deleted_at IS NULL)",
            )
            .bind(&slug)
            .bind(pk)
            .fetch_one(&st.pool)
            .await?;
            if exists {
                return Ok(deny());
            }
            Ok(missing())
        }
    }
}

pub async fn patch_sticky(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny());
    }
    let Some(cur) = fetch_owned_sticky(&st.pool, &slug, pk, auth.0).await? else {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM stickies s JOIN workspaces w ON w.id = s.workspace_id \
             WHERE w.slug = $1 AND s.id = $2 AND s.deleted_at IS NULL)",
        )
        .bind(&slug)
        .bind(pk)
        .fetch_one(&st.pool)
        .await?;
        if exists {
            return Ok(deny());
        }
        return Ok(missing());
    };
    let html: String = match body.get("description_html") {
        None | Some(Value::Null) => cur.description_html.clone(),
        Some(Value::String(s)) => match clean_description_html(s) {
            Ok(h) => h,
            Err(_) => {
                return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "html content is not valid"}))));
            }
        },
        Some(_) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "html content is not valid"}))));
        }
    };
    let binary: Option<Vec<u8>> = match body.get("description_binary") {
        None | Some(Value::Null) => cur.description_binary.clone(),
        Some(Value::String(s)) => match decode_description_binary(s) {
            Ok(b) if b.is_empty() && s.is_empty() => None,
            Ok(b) => Some(b),
            Err(e) => {
                return Ok((StatusCode::BAD_REQUEST, Json(json!({"description_binary": [e]}))));
            }
        },
        Some(_) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"description_binary": ["Invalid binary data"]}))));
        }
    };
    let stripped = strip_tags_text(&html);
    let row: Option<StickyRow> = sqlx::query_as(&format!(
        "UPDATE stickies s SET name = COALESCE($1, name), description = COALESCE($2, description), \
         description_html = $3, description_stripped = $4, description_binary = $5, \
         logo_props = COALESCE($6, logo_props), color = COALESCE($7, color), \
         background_color = COALESCE($8, background_color), \
         sort_order = COALESCE($9, sort_order), updated_at = now(), updated_by_id = $10 \
         FROM workspaces w WHERE w.id = s.workspace_id AND w.slug = $11 AND s.id = $12 \
         AND s.deleted_at IS NULL RETURNING {STICKY_COLS}",
    ))
    .bind(body.get("name").and_then(Value::as_str))
    .bind(body.get("description").cloned())
    .bind(&html)
    .bind(if stripped.is_empty() { None } else { Some(stripped) })
    .bind(&binary)
    .bind(body.get("logo_props").cloned())
    .bind(body.get("color").and_then(Value::as_str))
    .bind(body.get("background_color").and_then(Value::as_str))
    .bind(body.get("sort_order").and_then(Value::as_f64))
    .bind(auth.0)
    .bind(&slug)
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(sticky_json(&r)))),
        None => Ok(missing()),
    }
}

pub async fn delete_sticky(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny());
    }
    let Some(cur) = fetch_owned_sticky(&st.pool, &slug, pk, auth.0).await? else {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM stickies s JOIN workspaces w ON w.id = s.workspace_id \
             WHERE w.slug = $1 AND s.id = $2 AND s.deleted_at IS NULL)",
        )
        .bind(&slug)
        .bind(pk)
        .fetch_one(&st.pool)
        .await?;
        if exists {
            return Ok(deny());
        }
        return Ok(missing());
    };
    let _ = cur;
    sqlx::query(
        "UPDATE stickies s SET deleted_at = now() FROM workspaces w WHERE w.id = s.workspace_id AND w.slug = $1 AND s.id = $2 AND s.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(pk)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}
