use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Mirrors `plane/app/views/page/base.py:PageViewSet` list/create/summary
/// for `plane/app/urls/page.py`. Django `Page.name` is `TextField(blank=True)`
/// so untitled pages are allowed; `access` 0 Public / 1 Private; `color` 255.
/// Create links the page to the project via `project_pages` and stores
/// `description_html` default `<p></p>`. Archive/lock/access/versions/
/// duplicate/description are detail endpoints (later task).
#[derive(Debug, Clone, Deserialize)]
pub struct CreatePage {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub access: Option<i16>,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PageOut {
    pub id: uuid::Uuid,
    pub name: String,
}

pub fn validate_create(body: &CreatePage) -> Result<(), String> {
    if let Some(access) = body.access {
        if access != 0 && access != 1 {
            return Err("access must be 0 (Public) or 1 (Private)".to_string());
        }
    }
    if let Some(color) = &body.color {
        if color.chars().count() > 255 {
            return Err("color max length 255".to_string());
        }
    }
    Ok(())
}

async fn owner_id(st: &AppState, auth: &AuthUser) -> Result<uuid::Uuid, (StatusCode, Json<Value>)> {
    let id: Option<uuid::Uuid> = sqlx::query_scalar("SELECT user_id FROM api_tokens WHERE token = $1")
        .bind(&auth.0)
        .fetch_optional(&st.pool)
        .await
        .map_err(|_| (StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid api key"}))))?;
    id.ok_or((StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid api key"}))))
}

pub async fn list(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<PageOut>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::page::Page>(
        "SELECT p.id, p.name FROM pages p JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL JOIN workspaces w ON w.id = p.workspace_id WHERE w.slug = $1 AND pp.project_id = $2 AND p.deleted_at IS NULL ORDER BY p.created_at DESC",
    )
    .bind(&slug)
    .bind(project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(rows.into_iter().map(|p| PageOut { id: p.id, name: p.name }).collect()))
}

/// Mirrors `PageViewSet.summary`: {public_pages, private_pages, archived_pages}.
pub async fn summary(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Value>, common::errors::AppError> {
    let row: (i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE p.access = 0 AND p.archived_at IS NULL), COUNT(*) FILTER (WHERE p.access = 1 AND p.archived_at IS NULL), COUNT(*) FILTER (WHERE p.archived_at IS NOT NULL) FROM pages p JOIN project_pages pp ON pp.page_id = p.id AND pp.deleted_at IS NULL JOIN workspaces w ON w.id = p.workspace_id WHERE w.slug = $1 AND pp.project_id = $2 AND p.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(project_id)
    .fetch_one(&st.pool)
    .await?;
    Ok(Json(json!({"public_pages": row.0, "private_pages": row.1, "archived_pages": row.2})))
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreatePage>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    let owner = owner_id(&st, &auth).await.map_err(|(c, j)| anyhow::anyhow!("{}: {}", c, j.0))?;
    let access = body.access.unwrap_or(0);

    let page: common::models::page::Page = sqlx::query_as(
        "INSERT INTO pages (id, name, description_html, description_json, color, access, is_locked, is_global, owned_by_id, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), $1, '<p></p>', '{}', $2, $3, false, false, $4, w.id, now(), now() FROM workspaces w WHERE w.slug = $5 RETURNING id, name",
    )
    .bind(body.name.clone().unwrap_or_default())
    .bind(body.color.clone().unwrap_or_default())
    .bind(access)
    .bind(owner)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;

    sqlx::query(
        "INSERT INTO project_pages (id, project_id, page_id, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, w.id, now(), now() FROM workspaces w WHERE w.slug = $3",
    )
    .bind(project_id)
    .bind(page.id)
    .bind(&slug)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({"id": page.id, "name": page.name}))))
}

/// Mirrors PageFavoriteViewSet.create: POST .../favorite-pages/<page_id>/ → 204.
pub async fn create_favorite(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, page_id)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let owner = owner_id(&st, &auth).await.map_err(|(c, j)| anyhow::anyhow!("{}: {}", c, j.0))?;
    sqlx::query(
        "INSERT INTO user_favorites (id, entity_type, entity_identifier, user_id, is_folder, sequence, project_id, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), 'page', $1, $2, false, 65535, $3, w.id, now(), now() FROM workspaces w WHERE w.slug = $4 ON CONFLICT DO NOTHING",
    )
    .bind(page_id)
    .bind(owner)
    .bind(project_id)
    .bind(&slug)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}
