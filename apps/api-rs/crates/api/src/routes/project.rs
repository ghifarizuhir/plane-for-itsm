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
        "INSERT INTO projects (id, name, description, identifier, workspace_id, network, module_view, cycle_view, issue_views_view, page_view, intake_view, is_time_tracking_enabled, is_issue_type_enabled, guest_view_all_features, archive_in, close_in, logo_props, timezone, created_at, updated_at) SELECT gen_random_uuid(), $1, '', $2, w.id, 2, false, false, false, true, false, false, false, false, 0, 0, '{}', w.timezone, now(), now() FROM workspaces w WHERE w.slug = $3 RETURNING id, name",
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

/// Mirrors `plane/app/views/project/base.py:partial_update`: archived
/// projects are immutable.
pub fn guard_patch(archived: bool) -> Result<(), String> {
    if archived {
        return Err("Archived projects cannot be updated".to_string());
    }
    Ok(())
}

/// Mirrors `plane/app/serializers/project.py:validate_name` /
/// `validate_identifier`: sibling-name/identifier collisions are rejected.
pub fn guard_name_unique(exists: bool) -> Result<(), String> {
    if exists {
        return Err("PROJECT_NAME_ALREADY_EXIST".to_string());
    }
    Ok(())
}

pub fn guard_identifier_unique(exists: bool) -> Result<(), String> {
    if exists {
        return Err("PROJECT_IDENTIFIER_ALREADY_EXIST".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchProject {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub identifier: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

pub async fn detail(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    let row: Option<common::models::project::Project> = sqlx::query_as(
        "SELECT id, name FROM projects p JOIN workspaces w ON w.id = p.workspace_id WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&_slug)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(p) => Ok((
            StatusCode::OK,
            Json(serde_json::json!({"id": p.id, "name": p.name})),
        )),
        None => Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Project does not exist"})),
        )),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<PatchProject>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    let row: Option<(Option<chrono::DateTime<chrono::Utc>>, uuid::Uuid)> = sqlx::query_as(
        "SELECT p.archived_at, p.workspace_id FROM projects p JOIN workspaces w ON w.id = p.workspace_id WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some((archived_at, workspace_id)) = row else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Project does not exist"})),
        ));
    };
    if let Err(e) = guard_patch(archived_at.is_some()) {
        return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))));
    }
    if let Some(name) = &body.name {
        if name.trim().is_empty() || name.chars().count() > 255 || has_forbidden(name) {
            return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid name"}))));
        }
        let dup: Option<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT id FROM projects WHERE workspace_id = $1 AND name = $2 AND id != $3 AND deleted_at IS NULL",
        )
        .bind(workspace_id)
        .bind(name)
        .bind(pk)
        .fetch_optional(&st.pool)
        .await?;
        if let Err(e) = guard_name_unique(dup.is_some()) {
            return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))));
        }
    }
    if let Some(identifier) = &body.identifier {
        let ident = identifier.trim().to_uppercase();
        if ident.is_empty() || ident.chars().count() > 12 || has_forbidden(&ident) {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid identifier"})),
            ));
        }
        let dup: Option<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT id FROM projects WHERE workspace_id = $1 AND identifier = $2 AND id != $3 AND deleted_at IS NULL",
        )
        .bind(workspace_id)
        .bind(&ident)
        .bind(pk)
        .fetch_optional(&st.pool)
        .await?;
        if let Err(e) = guard_identifier_unique(dup.is_some()) {
            return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))));
        }
        sqlx::query(
            "UPDATE projects SET name = COALESCE($1, name), identifier = $2, description = COALESCE($3, description), updated_at = now() WHERE id = $4",
        )
        .bind(&body.name)
        .bind(&ident)
        .bind(&body.description)
        .bind(pk)
        .execute(&st.pool)
        .await?;
    } else {
        sqlx::query(
            "UPDATE projects SET name = COALESCE($1, name), description = COALESCE($2, description), updated_at = now() WHERE id = $3",
        )
        .bind(&body.name)
        .bind(&body.description)
        .bind(pk)
        .execute(&st.pool)
        .await?;
    }
    Ok((StatusCode::OK, Json(serde_json::json!({"id": pk}))))
}

pub async fn destroy(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    let n = sqlx::query(
        "UPDATE projects SET deleted_at = now() WHERE id = $1 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $2) AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&slug)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Project does not exist"})),
        ));
    }
    Ok((StatusCode::NO_CONTENT, Json(serde_json::json!(null))))
}
