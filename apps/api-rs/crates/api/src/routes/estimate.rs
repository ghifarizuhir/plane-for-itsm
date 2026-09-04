use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Mirrors `plane/app/serializers/estimate.py:EstimateSerializer` /
/// `plane/api/serializers/estimate.py` served by both
/// `plane/app/urls/estimate.py` (BulkEstimatePointEndpoint list/create)
/// and `plane/api/urls/estimate.py` (ProjectEstimateAPIEndpoint).
/// Unique (name, project) → 409 mirrors the constraint
/// `estimate_unique_name_project_when_deleted_at_null`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateEstimate {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "type")]
    pub estimate_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EstimateOut {
    pub id: uuid::Uuid,
    pub name: String,
}

/// Mirrors `EstimatePointSerializer.validate`:
/// empty payload rejected, value max 20 chars; create additionally
/// requires key+value ("Key and value are required" in
/// `plane/app/views/estimate/base.py:159`).
#[derive(Debug, Clone, Deserialize)]
pub struct CreateEstimatePoint {
    #[serde(default)]
    pub key: Option<i32>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

pub fn validate_create(body: &CreateEstimate) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    if let Some(t) = &body.estimate_type {
        if t != "categories" && t != "points" {
            return Err("type must be one of: categories, points".to_string());
        }
    }
    Ok(())
}

pub fn validate_point_create(body: &CreateEstimatePoint) -> Result<(), String> {
    match (&body.key, &body.value) {
        (Some(_), Some(v)) if !v.trim().is_empty() => {}
        _ => return Err("Key and value are required".to_string()),
    }
    if let Some(v) = &body.value {
        if v.chars().count() > 20 {
            return Err("Value can't be more than 20 characters".to_string());
        }
    }
    Ok(())
}

pub async fn list(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, _project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<EstimateOut>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::estimate::Estimate>(
        "SELECT id, name FROM estimates WHERE project_id = $1 AND deleted_at IS NULL ORDER BY name",
    )
    .bind(_project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|e| EstimateOut { id: e.id, name: e.name })
            .collect(),
    ))
}

pub async fn create(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateEstimate>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;

    let existing = sqlx::query_as::<_, common::models::estimate::Estimate>(
        "SELECT id, name FROM estimates WHERE project_id = $1 AND name = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(&body.name)
    .fetch_optional(&st.pool)
    .await?;
    if let Some(estimate) = existing {
        return Ok((
            StatusCode::CONFLICT,
            Json(json!({"error": "Estimate with the same name already exists in the project", "id": estimate.id})),
        ));
    }

    let estimate_type = body.estimate_type.as_deref().unwrap_or("categories");
    let row = sqlx::query_as::<_, common::models::estimate::Estimate>(
        "INSERT INTO estimates (id, name, description, type, project_id, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, $3, $4, w.id, now(), now() FROM workspaces w WHERE w.slug = $5 RETURNING id, name",
    )
    .bind(&body.name)
    .bind(body.description.clone().unwrap_or_default())
    .bind(estimate_type)
    .bind(project_id)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({"id": row.id, "name": row.name}))))
}

pub async fn create_point(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, _project_id, estimate_id)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<CreateEstimatePoint>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_point_create(&body).map_err(|e| anyhow::anyhow!(e))?;

    let estimate = sqlx::query_as::<_, common::models::estimate::Estimate>(
        "SELECT id, name FROM estimates WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(estimate_id)
    .fetch_optional(&st.pool)
    .await?;
    if estimate.is_none() {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Estimate not found"}))));
    }

    let row = sqlx::query_as::<_, common::models::estimate::EstimatePoint>(
        "INSERT INTO estimate_points (id, estimate_id, key, value, description, project_id, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, $3, $4, e.project_id, e.workspace_id, now(), now() FROM estimates e WHERE e.id = $1 RETURNING id, key, value",
    )
    .bind(estimate_id)
    .bind(body.key)
    .bind(&body.value)
    .bind(body.description.clone().unwrap_or_default())
    .fetch_one(&st.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"id": row.id, "key": row.key, "value": row.value})),
    ))
}
