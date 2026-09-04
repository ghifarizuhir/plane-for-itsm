use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Mirrors `plane/app/serializers/state.py:StateSerializer.validate`
/// + `plane/db/models/state.py:StateGroup`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateState {
    pub name: String,
    pub group: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StateOut {
    pub id: uuid::Uuid,
    pub name: String,
    pub group: String,
}

pub const ALLOWED_GROUPS: &[&str] = &[
    "backlog",
    "unstarted",
    "started",
    "completed",
    "cancelled",
];

pub fn validate_create(body: &CreateState) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    if body.group == "triage" {
        return Err("Cannot create triage state".to_string());
    }
    if !ALLOWED_GROUPS.contains(&body.group.as_str()) {
        return Err(format!("unknown group {}", body.group));
    }
    if body.color.trim().is_empty() {
        return Err("color is required".to_string());
    }
    Ok(())
}

pub async fn list(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, _project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<StateOut>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::state::State>(
        "SELECT id, name, \"group\" FROM states WHERE project_id = $1 AND deleted_at IS NULL ORDER BY sequence ASC",
    )
    .bind(_project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|s| StateOut {
                id: s.id,
                name: s.name,
                group: s.group,
            })
            .collect(),
    ))
}

pub async fn create(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateState>,
) -> Result<(StatusCode, Json<StateOut>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    let row = sqlx::query_as::<_, common::models::state::State>(
        "INSERT INTO states (id, name, \"group\", color, project_id, created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, now(), now()) RETURNING id, name, \"group\"",
    )
    .bind(&body.name)
    .bind(&body.group)
    .bind(&body.color)
    .bind(project_id)
    .fetch_one(&st.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(StateOut {
            id: row.id,
            name: row.name,
            group: row.group,
        }),
    ))
}
