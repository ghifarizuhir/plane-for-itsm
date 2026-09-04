use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Mirrors `plane/app/serializers/project.py:ProjectSerializer`
/// + `plane/db/models/project.py:FORBIDDEN_IDENTIFIER_CHARS_PATTERN`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateProject {
    pub name: String,
    pub identifier: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectOut {
    pub id: uuid::Uuid,
    pub name: String,
    pub identifier: String,
}

const FORBIDDEN: &[char] = &[
    '&', '+', ',', ':', ';', '$', '^', '}', '{', '*', '=', '?', '@', '#', '|', '\'', '<', '>',
    '.', '(', ')', '%', '!', '-', '/',
];

fn has_forbidden(s: &str) -> bool {
    s.chars().any(|c| FORBIDDEN.contains(&c))
}

pub fn validate_create(body: &CreateProject) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.identifier.trim().is_empty() {
        return Err("identifier is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    if body.identifier.chars().count() > 12 {
        return Err("identifier max length 12".to_string());
    }
    if has_forbidden(&body.name) {
        return Err("PROJECT_NAME_CANNOT_CONTAIN_SPECIAL_CHARACTERS".to_string());
    }
    if has_forbidden(&body.identifier) {
        return Err("PROJECT_IDENTIFIER_CANNOT_CONTAIN_SPECIAL_CHARACTERS".to_string());
    }
    Ok(())
}

pub async fn list(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<Json<Vec<ProjectOut>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::project::Project>(
        "SELECT p.id, p.name FROM projects p JOIN workspaces w ON w.id = p.workspace_id WHERE w.slug = $1 AND p.deleted_at IS NULL ORDER BY p.name ASC",
    )
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|p| ProjectOut {
                id: p.id,
                name: p.name,
                identifier: String::new(),
            })
            .collect(),
    ))
}

pub async fn create(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Json(body): Json<CreateProject>,
) -> Result<(StatusCode, Json<ProjectOut>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    let ident = body.identifier.trim().to_uppercase();
    let row = sqlx::query_as::<_, common::models::project::Project>(
        "INSERT INTO projects (id, name, description, identifier, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), $1, '', $2, w.id, now(), now() FROM workspaces w WHERE w.slug = $3 RETURNING id, name",
    )
    .bind(&body.name)
    .bind(&ident)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(ProjectOut {
            id: row.id,
            name: row.name,
            identifier: ident,
        }),
    ))
}
