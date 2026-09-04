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
        "INSERT INTO modules (id, name, project_id, start_date, target_date, created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now()) RETURNING id, name",
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
