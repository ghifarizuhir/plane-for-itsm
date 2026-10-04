//! CRUD `intake_sources` — sumber webhook inbound per project.
//! Endpoint ingest publiknya ada di `routes/inbound.rs`.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    middleware::auth::AuthUser,
    routes::{
        issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows},
        project::{deny, missing},
    },
    state::AppState,
};

const PRIORITIES: [&str; 5] = ["low", "medium", "high", "urgent", "none"];

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IntakeSourceRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub token: String,
    pub is_active: bool,
    pub auto_accept: bool,
    pub type_id: Option<Uuid>,
    pub config: Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by_id: Option<Uuid>,
}

const SOURCE_COLS: &str = "s.id, s.project_id, s.name, s.token, s.is_active, s.auto_accept, \
    s.type_id, s.config, s.created_at, s.updated_at, s.created_by_id";

fn source_json(row: &IntakeSourceRow) -> Value {
    json!({
        "id": row.id,
        "project_id": row.project_id,
        "name": row.name,
        "token": row.token,
        "is_active": row.is_active,
        "auto_accept": row.auto_accept,
        "type_id": row.type_id,
        "config": row.config,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "created_by": row.created_by_id,
    })
}

#[derive(Debug, Deserialize)]
pub struct CreateIntakeSource {
    pub name: String,
    #[serde(default)]
    pub type_id: Option<Uuid>,
    #[serde(default)]
    pub auto_accept: Option<bool>,
    #[serde(default)]
    pub config: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct PatchIntakeSource {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub type_id: Option<Uuid>,
    #[serde(default)]
    pub auto_accept: Option<bool>,
    #[serde(default)]
    pub is_active: Option<bool>,
    #[serde(default)]
    pub config: Option<Value>,
}

/// Validasi config mapping. `project_id` dipakai untuk memastikan service
/// milik project yang sama. Return `Err(pesan)` untuk 400.
pub async fn validate_config(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    config: &Value,
) -> Result<(), String> {
    for key in ["service_label_key", "severity_label_key"] {
        if let Some(value) = config.get(key) {
            if value.as_str().map(str::trim).unwrap_or("").is_empty() {
                return Err(format!("{key} must not be empty"));
            }
        }
    }
    let mut service_ids: Vec<Uuid> = Vec::new();
    if let Some(map) = config.get("service_map") {
        let Some(map) = map.as_object() else {
            return Err("service_map must be an object".into());
        };
        for (key, value) in map {
            if key.trim().is_empty() {
                return Err("service_map label value must not be empty".into());
            }
            let Some(id) = value.as_str().and_then(|s| Uuid::parse_str(s).ok()) else {
                return Err("service_map values must be service ids".into());
            };
            service_ids.push(id);
        }
    }
    if let Some(raw) = config.get("fallback_service_id").and_then(Value::as_str) {
        let Ok(id) = Uuid::parse_str(raw) else {
            return Err("Invalid fallback_service_id".into());
        };
        service_ids.push(id);
    }
    for service_id in service_ids {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM services WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)",
        )
        .bind(service_id)
        .bind(project_id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;
        if !exists {
            return Err("Service does not belong to this project".into());
        }
    }
    if let Some(map) = config.get("severity_map").and_then(Value::as_object) {
        for (key, value) in map {
            if key.trim().is_empty() {
                return Err("severity_map label value must not be empty".into());
            }
            let Some(priority) = value.as_str() else {
                return Err("severity_map values must be priorities".into());
            };
            if !PRIORITIES.contains(&priority) {
                return Err("Invalid priority".into());
            }
        }
    }
    if let Some(priority) = config.get("default_priority").and_then(Value::as_str) {
        if !PRIORITIES.contains(&priority) {
            return Err("Invalid priority".into());
        }
    }
    Ok(())
}

async fn writer_gate(
    pool: &sqlx::PgPool,
    user: Uuid,
    slug: &str,
    project_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(
        matches!(role, Some(20) | Some(15)),
        role.is_some(),
        ws_admin,
    ))
}

async fn member_gate(
    pool: &sqlx::PgPool,
    user: Uuid,
    slug: &str,
    project_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(
        matches!(role, Some(20) | Some(15) | Some(5)),
        role.is_some(),
        ws_admin,
    ))
}

/// Type harus live, non-epic, dan ter-link ke project.
async fn valid_type(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    type_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issue_types t \
         JOIN project_issue_types pit ON pit.issue_type_id = t.id AND pit.deleted_at IS NULL \
         WHERE t.id = $1 AND pit.project_id = $2 AND t.deleted_at IS NULL AND t.is_epic = false)",
    )
    .bind(type_id)
    .bind(project_id)
    .fetch_one(pool)
    .await
}

pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !member_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let rows: Vec<IntakeSourceRow> = sqlx::query_as(&format!(
        "SELECT {SOURCE_COLS} FROM intake_sources s \
         WHERE s.project_id = $1 AND s.deleted_at IS NULL ORDER BY s.created_at ASC"
    ))
    .bind(project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(source_json).collect())),
    ))
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
    Json(body): Json<CreateIntakeSource>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !writer_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Name is required"})),
        ));
    }
    let Some(type_id) = body.type_id else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Select a work item type"})),
        ));
    };
    if !valid_type(&st.pool, project_id, type_id).await? {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Invalid work item type"})),
        ));
    }
    let config = body.config.unwrap_or_else(|| json!({}));
    if let Err(message) = validate_config(&st.pool, project_id, &config).await {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
    }
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO intake_sources (id, project_id, name, token, is_active, auto_accept, type_id, \
         config, created_by_id, updated_by_id, created_at, updated_at) \
         VALUES ($1, $2, $3, 'plane_is_' || replace(gen_random_uuid()::text, '-', ''), true, $4, $5, $6, $7, $7, now(), now())",
    )
    .bind(id)
    .bind(project_id)
    .bind(&name)
    .bind(body.auto_accept.unwrap_or(false))
    .bind(type_id)
    .bind(&config)
    .bind(auth.0)
    .execute(&st.pool)
    .await?;
    let row: IntakeSourceRow =
        sqlx::query_as(&format!(
            "SELECT {SOURCE_COLS} FROM intake_sources s WHERE s.id = $1"
        ))
        .bind(id)
        .fetch_one(&st.pool)
        .await?;
    Ok((StatusCode::CREATED, Json(source_json(&row))))
}

async fn fetch_source(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    pk: Uuid,
) -> Result<Option<IntakeSourceRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {SOURCE_COLS} FROM intake_sources s \
         WHERE s.id = $1 AND s.project_id = $2 AND s.deleted_at IS NULL"
    ))
    .bind(pk)
    .bind(project_id)
    .fetch_optional(pool)
    .await
}

pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !member_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    match fetch_source(&st.pool, project_id, pk).await? {
        Some(row) => Ok((StatusCode::OK, Json(source_json(&row)))),
        None => Ok(missing()),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<PatchIntakeSource>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !writer_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let Some(current) = fetch_source(&st.pool, project_id, pk).await? else {
        return Ok(missing());
    };
    let name = body.name.as_ref().map(|n| n.trim().to_string());
    if matches!(&name, Some(n) if n.is_empty()) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Name is required"})),
        ));
    }
    if let Some(type_id) = body.type_id {
        if !valid_type(&st.pool, project_id, type_id).await? {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Invalid work item type"})),
            ));
        }
    }
    if let Some(config) = &body.config {
        if let Err(message) = validate_config(&st.pool, project_id, config).await {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": message}))));
        }
    }
    let config = body.config.unwrap_or(current.config);
    sqlx::query(
        "UPDATE intake_sources SET name = COALESCE($1, name), type_id = COALESCE($2, type_id), \
         auto_accept = COALESCE($3, auto_accept), is_active = COALESCE($4, is_active), \
         config = $5, updated_by_id = $6, updated_at = now() WHERE id = $7 AND deleted_at IS NULL",
    )
    .bind(name)
    .bind(body.type_id)
    .bind(body.auto_accept)
    .bind(body.is_active)
    .bind(&config)
    .bind(auth.0)
    .bind(pk)
    .execute(&st.pool)
    .await?;
    let row = fetch_source(&st.pool, project_id, pk).await?.unwrap();
    Ok((StatusCode::OK, Json(source_json(&row))))
}

pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !writer_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let result = sqlx::query(
        "UPDATE intake_sources SET deleted_at = now(), updated_by_id = $1, updated_at = now() \
         WHERE id = $2 AND project_id = $3 AND deleted_at IS NULL",
    )
    .bind(auth.0)
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(missing());
    }
    Ok((StatusCode::OK, Json(json!({"ok": true}))))
}

pub async fn rotate(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !writer_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let token: Option<String> = sqlx::query_scalar(
        "UPDATE intake_sources SET token = 'plane_is_' || replace(gen_random_uuid()::text, '-', ''), \
         updated_by_id = $1, updated_at = now() \
         WHERE id = $2 AND project_id = $3 AND deleted_at IS NULL RETURNING token",
    )
    .bind(auth.0)
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    match token {
        Some(token) => Ok((StatusCode::OK, Json(json!({"id": pk, "token": token})))),
        None => Ok(missing()),
    }
}
