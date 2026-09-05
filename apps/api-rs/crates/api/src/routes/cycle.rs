use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Mirrors `plane/app/serializers/cycle.py:CycleWriteSerializer.validate`
/// + #9200 guard (archive requires end_date).
#[derive(Debug, Clone, Deserialize)]
pub struct CreateCycle {
    pub name: String,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CycleOut {
    pub id: uuid::Uuid,
    pub name: String,
}

pub fn validate_create(body: &CreateCycle) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    if let (Some(s), Some(e)) = (body.start_date, body.end_date) {
        if s > e {
            return Err("Start date cannot exceed end date".to_string());
        }
    }
    if body.start_date.is_some() != body.end_date.is_some() {
        return Err("Both start date and end date are either required or are to be null".to_string());
    }
    Ok(())
}

/// Detail guards from `plane/app/views/cycle/base.py:partial_update`:
/// archived cycles are immutable; completed cycles (end_date past) accept
/// only sort-order changes.
pub fn guard_patch(archived: bool, completed: bool, sort_only: bool) -> Result<(), String> {
    if archived {
        return Err("Archived cycle cannot be updated".to_string());
    }
    if completed && !sort_only {
        return Err("The Cycle has already been completed so it cannot be edited".to_string());
    }
    Ok(())
}

/// #9200: archiving without end_date must fail.
pub fn validate_archive(end_date: Option<DateTime<Utc>>) -> Result<(), String> {
    end_date.ok_or_else(|| "end_date is required when archiving cycle".to_string())?;
    Ok(())
}

pub async fn list(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, _project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<CycleOut>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::cycle::Cycle>(
        "SELECT id, name FROM cycles WHERE project_id = $1 AND deleted_at IS NULL ORDER BY created_at DESC",
    )
    .bind(_project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|c| CycleOut { id: c.id, name: c.name })
            .collect(),
    ))
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((_slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateCycle>,
) -> Result<(StatusCode, Json<CycleOut>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    // Django `Cycle.save` (`plane/db/models/cycle.py:70-95`): sort_order =
    // min-10000 per project; owned_by = request user; version 1.
    let owner = auth.0;
    let row = sqlx::query_as::<_, common::models::cycle::Cycle>(
        "INSERT INTO cycles (id, name, description, project_id, workspace_id, owned_by_id, timezone, version, view_props, logo_props, progress_snapshot, sort_order, start_date, end_date, created_at, updated_at) SELECT gen_random_uuid(), $1, '', p.id, p.workspace_id, $2, 'UTC', 1, '{}', '{}', '{}', COALESCE((SELECT MIN(sort_order) FROM cycles WHERE project_id = p.id), 65535 + 10000) - 10000, $3, $4, now(), now() FROM projects p WHERE p.id = $5 RETURNING id, name",
    )
    .bind(&body.name)
    .bind(owner)
    .bind(body.start_date)
    .bind(body.end_date)
    .bind(project_id)
    .fetch_one(&st.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(CycleOut { id: row.id, name: row.name }),
    ))
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchCycle {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub start_date: Option<DateTime<Utc>>,
    #[serde(default)]
    pub end_date: Option<DateTime<Utc>>,
    #[serde(default)]
    pub sort_order: Option<f64>,
}

pub async fn detail(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    let row: Option<common::models::cycle::Cycle> = sqlx::query_as(
        "SELECT id, name FROM cycles WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(c) => Ok((StatusCode::OK, Json(serde_json::json!({"id": c.id, "name": c.name})))),
        None => Ok((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Cycle not found"})))),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchCycle>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    let row: Option<(Option<DateTime<Utc>>, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT archived_at, end_date FROM cycles WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some((archived_at, end_date)) = row else {
        return Ok((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Cycle not found"}))));
    };
    let completed = end_date.map(|e| e < Utc::now()).unwrap_or(false);
    let sort_only = body.name.is_none() && body.start_date.is_none() && body.end_date.is_none();
    if let Err(e) = guard_patch(archived_at.is_some(), completed, sort_only) {
        return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))));
    }
    if body.start_date.is_some() != body.end_date.is_some() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Both start date and end date are either required or are to be null"})),
        ));
    }
    if let (Some(s), Some(e)) = (body.start_date, body.end_date) {
        if s > e {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Start date cannot exceed end date"})),
            ));
        }
    }
    if let Some(name) = &body.name {
        if name.trim().is_empty() || name.chars().count() > 255 {
            return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid name"}))));
        }
    }
    sqlx::query(
        "UPDATE cycles SET name = COALESCE($1, name), start_date = COALESCE($2, start_date), end_date = COALESCE($3, end_date), updated_at = now() WHERE id = $4",
    )
    .bind(&body.name)
    .bind(body.start_date)
    .bind(body.end_date)
    .bind(pk)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(serde_json::json!({"id": pk}))))
}

pub async fn destroy(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    sqlx::query(
        "UPDATE cycles SET deleted_at = now() WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(serde_json::json!(null))))
}
