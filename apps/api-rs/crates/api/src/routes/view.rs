use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Mirrors `plane/app/views/view/` (IssueViewViewSet list/create +
/// WorkspaceViewViewSet list/create) for `plane/app/urls/views.py`:
/// project views (`.../views/`, project_id set) and global views
/// (`workspaces/:slug/views/`, project NULL). `name` required/255,
/// `access` 0 (Private) / 1 (Public). Owner/lock guards ("view is
/// locked", "Only admin or owner can delete the view") belong to detail.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateView {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub access: Option<i16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ViewOut {
    pub id: uuid::Uuid,
    pub name: String,
}

/// Mirrors IssueViewFavoriteViewSet.create: body `{view: <uuid>}`
/// stored as UserFavorite(entity_type="view").
#[derive(Debug, Clone, Deserialize)]
pub struct CreateFavorite {
    #[serde(default)]
    pub view: Option<uuid::Uuid>,
}

pub fn validate_create(body: &CreateView) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    if let Some(access) = body.access {
        if access != 0 && access != 1 {
            return Err("access must be 0 (Private) or 1 (Public)".to_string());
        }
    }
    Ok(())
}

pub fn validate_favorite_create(body: &CreateFavorite) -> Result<(), String> {
    if body.view.is_none() {
        return Err("view is required".to_string());
    }
    Ok(())
}

/// Django always has a real request.user; the strangler resolves the owner
/// from `api_tokens` (X-Api-Key path). Unresolvable tokens → 401.
async fn owner_id(st: &AppState, auth: &AuthUser) -> Result<uuid::Uuid, (StatusCode, Json<Value>)> {
    let id: Option<uuid::Uuid> = sqlx::query_scalar("SELECT user_id FROM api_tokens WHERE token = $1")
        .bind(&auth.0)
        .fetch_optional(&st.pool)
        .await
        .map_err(|_| (StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid api key"}))))?;
    id.ok_or((StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid api key"}))))
}

async fn list_views(
    st: &AppState,
    slug: &str,
    project_id: Option<uuid::Uuid>,
) -> Result<Vec<ViewOut>, common::errors::AppError> {
    let rows = match project_id {
        Some(pid) => sqlx::query_as::<_, common::models::view::IssueView>(
            "SELECT v.id, v.name FROM issue_views v JOIN workspaces w ON w.id = v.workspace_id WHERE w.slug = $1 AND v.project_id = $2 AND v.deleted_at IS NULL ORDER BY v.created_at DESC",
        )
        .bind(slug)
        .bind(pid)
        .fetch_all(&st.pool)
        .await?,
        None => sqlx::query_as::<_, common::models::view::IssueView>(
            "SELECT v.id, v.name FROM issue_views v JOIN workspaces w ON w.id = v.workspace_id WHERE w.slug = $1 AND v.project_id IS NULL AND v.deleted_at IS NULL ORDER BY v.created_at DESC",
        )
        .bind(slug)
        .fetch_all(&st.pool)
        .await?,
    };
    Ok(rows.into_iter().map(|v| ViewOut { id: v.id, name: v.name }).collect())
}

pub async fn list(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<ViewOut>>, common::errors::AppError> {
    Ok(Json(list_views(&st, &slug, Some(project_id)).await?))
}

pub async fn list_global(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<Json<Vec<ViewOut>>, common::errors::AppError> {
    Ok(Json(list_views(&st, &slug, None).await?))
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateView>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    let owner = owner_id(&st, &auth).await.map_err(|(c, j)| anyhow::anyhow!("{}: {}", c, j.0))?;
    let access = body.access.unwrap_or(1);

    // NOT NULL JSON columns get '{}' (Django Python-side defaults).
    let row = sqlx::query_as::<_, common::models::view::IssueView>(
        "INSERT INTO issue_views (id, name, description, query, filters, display_filters, display_properties, rich_filters, logo_props, access, sort_order, is_locked, project_id, workspace_id, owned_by_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, '{}', '{}', '{}', '{}', '{}', '{}', $3, 65535, false, $4, w.id, $5, now(), now() FROM workspaces w WHERE w.slug = $6 RETURNING id, name",
    )
    .bind(&body.name)
    .bind(body.description.clone().unwrap_or_default())
    .bind(access)
    .bind(project_id)
    .bind(owner)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({"id": row.id, "name": row.name}))))
}

pub async fn create_global(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Json(body): Json<CreateView>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    let owner = owner_id(&st, &auth).await.map_err(|(c, j)| anyhow::anyhow!("{}: {}", c, j.0))?;
    let access = body.access.unwrap_or(1);

    let row = sqlx::query_as::<_, common::models::view::IssueView>(
        "INSERT INTO issue_views (id, name, description, query, filters, display_filters, display_properties, rich_filters, logo_props, access, sort_order, is_locked, project_id, workspace_id, owned_by_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, '{}', '{}', '{}', '{}', '{}', '{}', $3, 65535, false, NULL, w.id, $4, now(), now() FROM workspaces w WHERE w.slug = $5 RETURNING id, name",
    )
    .bind(&body.name)
    .bind(body.description.clone().unwrap_or_default())
    .bind(access)
    .bind(owner)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({"id": row.id, "name": row.name}))))
}

pub async fn list_favorites(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, _project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<Value>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::view::UserFavorite>(
        "SELECT id, entity_identifier FROM user_favorites WHERE project_id = $1 AND entity_type = 'view' AND deleted_at IS NULL ORDER BY created_at DESC",
    )
    .bind(_project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|f| json!({"id": f.id, "view": f.entity_identifier}))
            .collect(),
    ))
}

pub async fn create_favorite(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateFavorite>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_favorite_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    let owner = owner_id(&st, &auth).await.map_err(|(c, j)| anyhow::anyhow!("{}: {}", c, j.0))?;
    let view_id = body.view.unwrap();

    let existing: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM user_favorites WHERE project_id = $1 AND entity_type = 'view' AND entity_identifier = $2 AND user_id = $3 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(view_id)
    .bind(owner)
    .fetch_optional(&st.pool)
    .await?;
    if let Some(id) = existing {
        return Ok((StatusCode::CONFLICT, Json(json!({"error": "View already favorited", "id": id}))));
    }

    let row = sqlx::query_as::<_, common::models::view::UserFavorite>(
        "INSERT INTO user_favorites (id, entity_type, entity_identifier, user_id, is_folder, sequence, project_id, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), 'view', $1, $2, false, 65535, $3, w.id, now(), now() FROM workspaces w WHERE w.slug = $4 RETURNING id, entity_identifier",
    )
    .bind(view_id)
    .bind(owner)
    .bind(project_id)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({"id": row.id, "view": row.entity_identifier}))))
}
