use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Mirrors `plane/api/serializers/issue.py:LabelCreateUpdateSerializer`
/// served by `plane/api/urls/label.py` (LabelListCreateAPIEndpoint).
/// Uniqueness (project,name) and (external_source,external_id) → 409
/// mirrors the IntegrityError branch in `plane/api/views/issue.py:878`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateLabel {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub external_source: Option<String>,
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub sort_order: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LabelOut {
    pub id: uuid::Uuid,
    pub name: String,
}

pub fn validate_create(body: &CreateLabel) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    if let Some(color) = &body.color {
        if color.chars().count() > 255 {
            return Err("color max length 255".to_string());
        }
    }
    Ok(())
}

pub async fn list(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, _project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<LabelOut>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::label::Label>(
        "SELECT id, name FROM labels WHERE project_id = $1 AND deleted_at IS NULL ORDER BY created_at DESC",
    )
    .bind(_project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|l| LabelOut { id: l.id, name: l.name })
            .collect(),
    ))
}

pub async fn create(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateLabel>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;

    // External-id idempotency guard (Django returns the existing label with 409).
    if let (Some(source), Some(external_id)) = (&body.external_source, &body.external_id) {
        let existing = sqlx::query_as::<_, common::models::label::Label>(
            "SELECT id, name FROM labels WHERE project_id = $1 AND external_source = $2 AND external_id = $3 AND deleted_at IS NULL",
        )
        .bind(project_id)
        .bind(source)
        .bind(external_id)
        .fetch_optional(&st.pool)
        .await?;
        if let Some(label) = existing {
            return Ok((
                StatusCode::CONFLICT,
                Json(json!({"error": "Label with the same external id and external source already exists", "id": label.id})),
            ));
        }
    }

    // Unique (project, name) guard → 409 mirrors IntegrityError branch.
    let existing = sqlx::query_as::<_, common::models::label::Label>(
        "SELECT id, name FROM labels WHERE project_id = $1 AND name = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(&body.name)
    .fetch_optional(&st.pool)
    .await?;
    if let Some(label) = existing {
        return Ok((
            StatusCode::CONFLICT,
            Json(json!({"error": "Label with the same name already exists in the project", "id": label.id})),
        ));
    }

    // sort_order mirrors Label.save(): max + 10000, default 65535.
    let row = sqlx::query_as::<_, common::models::label::Label>(
        "INSERT INTO labels (id, name, color, description, external_source, external_id, parent_id, sort_order, project_id, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, $3, $4, $5, $6, COALESCE($7, (SELECT MAX(sort_order) + 10000 FROM labels WHERE project_id = $8), 65535), $8, w.id, now(), now() FROM workspaces w WHERE w.slug = $9 RETURNING id, name",
    )
    .bind(&body.name)
    .bind(body.color.clone().unwrap_or_default())
    .bind(body.description.clone().unwrap_or_default())
    .bind(&body.external_source)
    .bind(&body.external_id)
    .bind(body.parent_id)
    .bind(body.sort_order)
    .bind(project_id)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({"id": row.id, "name": row.name}))))
}

/// Mirrors `plane/app/views/issue/label.py:partial_update`: renaming onto an
/// existing sibling name is rejected with 400.
pub fn guard_patch(name_exists: bool) -> Result<(), String> {
    if name_exists {
        return Err("Label with the same name already exists in the project".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchLabel {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parent_id: Option<uuid::Uuid>,
}

pub async fn detail(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<common::models::label::Label> = sqlx::query_as(
        "SELECT id, name FROM labels WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(l) => Ok((StatusCode::OK, Json(json!({"id": l.id, "name": l.name})))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Label not found"})))),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchLabel>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if let Some(name) = &body.name {
        if name.trim().is_empty() || name.chars().count() > 255 {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid name"}))));
        }
        let dup: Option<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT id FROM labels WHERE project_id = $1 AND name = $2 AND id != $3 AND deleted_at IS NULL",
        )
        .bind(project_id)
        .bind(name)
        .bind(pk)
        .fetch_optional(&st.pool)
        .await?;
        if let Err(e) = guard_patch(dup.is_some()) {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
        }
    }
    let n = sqlx::query(
        "UPDATE labels SET name = COALESCE($1, name), color = COALESCE($2, color), description = COALESCE($3, description), updated_at = now() WHERE id = $4 AND project_id = $5 AND deleted_at IS NULL",
    )
    .bind(&body.name)
    .bind(&body.color)
    .bind(&body.description)
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Label not found"}))));
    }
    Ok((StatusCode::OK, Json(json!({"id": pk}))))
}

pub async fn destroy(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    sqlx::query(
        "UPDATE labels SET deleted_at = now() WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}
