use axum::{extract::State, http::StatusCode, Json};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Mirrors `plane/app/serializers/module.py:ModuleWriteSerializer.validate`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateModule {
    pub name: String,
    pub start_date: Option<NaiveDate>,
    pub target_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModuleOut {
    pub id: uuid::Uuid,
    pub name: String,
}

pub fn validate_create(body: &CreateModule) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    if let (Some(s), Some(t)) = (body.start_date, body.target_date) {
        if s > t {
            return Err("Start date cannot exceed target date".to_string());
        }
    }
    Ok(())
}

pub async fn list(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, _project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<ModuleOut>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::module::Module>(
        "SELECT id, name FROM modules WHERE project_id = $1 AND deleted_at IS NULL ORDER BY created_at DESC",
    )
    .bind(_project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|m| ModuleOut { id: m.id, name: m.name })
            .collect(),
    ))
}

pub async fn create(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateModule>,
) -> Result<(StatusCode, Json<ModuleOut>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    let row = sqlx::query_as::<_, common::models::module::Module>(
        "INSERT INTO modules (id, name, description, project_id, start_date, target_date, created_at, updated_at) VALUES (gen_random_uuid(), $1, '', $2, $3, $4, now(), now()) RETURNING id, name",
    )
    .bind(&body.name)
    .bind(project_id)
    .bind(body.start_date)
    .bind(body.target_date)
    .fetch_one(&st.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(ModuleOut { id: row.id, name: row.name }),
    ))
}

/// Mirrors `plane/app/views/module/base.py`: archived modules are immutable
/// ("Archived module cannot be updated"), missing → "Module not found".
pub fn guard_patch(archived: bool) -> Result<(), String> {
    if archived {
        return Err("Archived module cannot be updated".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchModule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub start_date: Option<NaiveDate>,
    #[serde(default)]
    pub target_date: Option<NaiveDate>,
}

pub async fn detail(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    let row: Option<common::models::module::Module> = sqlx::query_as(
        "SELECT id, name FROM modules WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(m) => Ok((StatusCode::OK, Json(serde_json::json!({"id": m.id, "name": m.name})))),
        None => Ok((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Module not found"})))),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchModule>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    let row: Option<(Option<chrono::DateTime<chrono::Utc>>,)> = sqlx::query_as(
        "SELECT archived_at FROM modules WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some((archived_at,)) = row else {
        return Ok((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Module not found"}))));
    };
    if let Err(e) = guard_patch(archived_at.is_some()) {
        return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))));
    }
    if let Some(name) = &body.name {
        if name.trim().is_empty() || name.chars().count() > 255 {
            return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid name"}))));
        }
    }
    if let (Some(s), Some(t)) = (body.start_date, body.target_date) {
        if s > t {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Start date cannot exceed target date"})),
            ));
        }
    }
    sqlx::query(
        "UPDATE modules SET name = COALESCE($1, name), start_date = COALESCE($2, start_date), target_date = COALESCE($3, target_date), updated_at = now() WHERE id = $4",
    )
    .bind(&body.name)
    .bind(body.start_date)
    .bind(body.target_date)
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
        "UPDATE modules SET deleted_at = now() WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(serde_json::json!(null))))
}
