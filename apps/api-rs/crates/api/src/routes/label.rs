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
    .bind(&body.color)
    .bind(&body.description)
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
