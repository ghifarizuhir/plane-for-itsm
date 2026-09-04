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
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateCycle>,
) -> Result<(StatusCode, Json<CycleOut>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    let row = sqlx::query_as::<_, common::models::cycle::Cycle>(
        "INSERT INTO cycles (id, name, project_id, start_date, end_date, created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now()) RETURNING id, name",
    )
    .bind(&body.name)
    .bind(project_id)
    .bind(body.start_date)
    .bind(body.end_date)
    .fetch_one(&st.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(CycleOut { id: row.id, name: row.name }),
    ))
}
