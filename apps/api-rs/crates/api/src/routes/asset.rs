use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Strangler boundary for `plane/app/urls/asset.py` (v2 endpoints):
///
/// IN SCOPE (metadata/control, pure Postgres):
/// - `GET assets/v2/workspaces/:slug/check/:asset_id/` → `{"exists": bool}`
///   (AssetCheckEndpoint).
/// - `POST assets/v2/workspaces/:slug/restore/:asset_id/` → 204
///   (AssetRestoreEndpoint: clears is_deleted/deleted_at).
/// - `PATCH assets/v2/workspaces/:slug/:asset_id/` → 204, marks
///   `is_uploaded` + stores attributes (upload-complete callback; the
///   celery metadata fetch is a worker concern, non-contract).
/// - `DELETE assets/v2/workspaces/:slug/:asset_id/` → 204 soft-delete
///   (is_deleted/deleted_at), missing id is a silent no-op like Django.
///
/// STAYS ON DJANGO: upload-init presigned POST
/// (`WorkspaceFileAssetEndpoint.post`) and download redirects
/// (`.../download/...`, static, project GET) — these sign S3 URLs and need
/// a dedicated S3-signing task. The upload-init *validation rules*
/// (entity allowlist, file-type allowlist, size clamp) are ported below so
/// the S3 task only adds signing.
pub const FILE_SIZE_LIMIT: i64 = 5_242_880;

pub const ENTITY_TYPES: [&str; 10] = [
    "ISSUE_ATTACHMENT",
    "ISSUE_DESCRIPTION",
    "COMMENT_DESCRIPTION",
    "PAGE_DESCRIPTION",
    "USER_COVER",
    "USER_AVATAR",
    "WORKSPACE_LOGO",
    "PROJECT_COVER",
    "DRAFT_ISSUE_ATTACHMENT",
    "DRAFT_ISSUE_DESCRIPTION",
];

pub const ALLOWED_FILE_TYPES: [&str; 5] = [
    "image/jpeg",
    "image/png",
    "image/webp",
    "image/jpg",
    "image/gif",
];

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAssetInit {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, rename = "type")]
    pub file_type: Option<String>,
    #[serde(default)]
    pub size: Option<i64>,
    #[serde(default)]
    pub entity_type: String,
    #[serde(default)]
    pub entity_identifier: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PatchAsset {
    #[serde(default)]
    pub attributes: Option<Value>,
}

pub fn validate_upload_init(body: &CreateAssetInit) -> Result<(), String> {
    if !ENTITY_TYPES.contains(&body.entity_type.as_str()) {
        return Err("Invalid entity type.".to_string());
    }
    let file_type = body.file_type.as_deref().unwrap_or("image/jpeg");
    if !ALLOWED_FILE_TYPES.contains(&file_type) {
        return Err("Invalid file type. Only JPEG, PNG, WebP, JPG and GIF files are allowed.".to_string());
    }
    Ok(())
}

/// Mirrors `size_limit = min(settings.FILE_SIZE_LIMIT, size)`.
pub fn clamp_size(size: i64) -> i64 {
    size.min(FILE_SIZE_LIMIT)
}

async fn user_id(st: &AppState, auth: &AuthUser) -> Option<uuid::Uuid> {
    sqlx::query_scalar("SELECT user_id FROM api_tokens WHERE token = $1")
        .bind(&auth.0)
        .fetch_optional(&st.pool)
        .await
        .ok()
        .flatten()
}

async fn find_asset(
    st: &AppState,
    slug: &str,
    asset_id: uuid::Uuid,
) -> Result<Option<common::models::asset::FileAsset>, common::errors::AppError> {
    Ok(sqlx::query_as::<_, common::models::asset::FileAsset>(
        "SELECT a.id, a.workspace_id, a.project_id, a.entity_type, a.is_uploaded FROM file_assets a JOIN workspaces w ON w.id = a.workspace_id WHERE w.slug = $1 AND a.id = $2 AND a.deleted_at IS NULL",
    )
    .bind(slug)
    .bind(asset_id)
    .fetch_optional(&st.pool)
    .await?)
}

/// Mirrors `has_project_asset_access`: project-bound assets require an
/// active ProjectMember row for the caller's user.
async fn check_project_access(
    st: &AppState,
    auth: &AuthUser,
    asset: &common::models::asset::FileAsset,
) -> Result<bool, common::errors::AppError> {
    let Some(project_id) = asset.project_id else {
        return Ok(true);
    };
    let Some(uid) = user_id(st, auth).await else {
        return Ok(false);
    };
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM project_members WHERE member_id = $1 AND workspace_id = $2 AND project_id = $3 AND is_active = true AND deleted_at IS NULL)",
    )
    .bind(uid)
    .bind(asset.workspace_id)
    .bind(project_id)
    .fetch_one(&st.pool)
    .await?;
    Ok(allowed)
}

pub async fn check(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, asset_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Value>, common::errors::AppError> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM file_assets a JOIN workspaces w ON w.id = a.workspace_id WHERE w.slug = $1 AND a.id = $2 AND a.deleted_at IS NULL)",
    )
    .bind(&slug)
    .bind(asset_id)
    .fetch_one(&st.pool)
    .await?;
    Ok(Json(json!({"exists": exists})))
}

pub async fn restore(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, asset_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let updated = sqlx::query(
        "UPDATE file_assets a SET is_deleted = false, deleted_at = NULL FROM workspaces w WHERE w.id = a.workspace_id AND w.slug = $1 AND a.id = $2",
    )
    .bind(&slug)
    .bind(asset_id)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if updated == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Asset not found"}))));
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

pub async fn mark_uploaded(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, asset_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<PatchAsset>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let Some(asset) = find_asset(&st, &slug, asset_id).await? else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Asset not found"}))));
    };
    if !check_project_access(&st, &auth, &asset).await? {
        return Ok((StatusCode::FORBIDDEN, Json(json!({"error": "You don't have access to this asset."}))));
    }
    sqlx::query("UPDATE file_assets SET is_uploaded = true, attributes = COALESCE($1, attributes) WHERE id = $2")
        .bind(&body.attributes)
        .bind(asset_id)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

pub async fn soft_delete(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, asset_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django's asset_delete is a silent no-op when the row is missing.
    if let Some(asset) = find_asset(&st, &slug, asset_id).await? {
        if !check_project_access(&st, &auth, &asset).await? {
            return Ok((StatusCode::FORBIDDEN, Json(json!({"error": "You don't have access to this asset."}))));
        }
        sqlx::query("UPDATE file_assets SET is_deleted = true, deleted_at = now() WHERE id = $1")
            .bind(asset_id)
            .execute(&st.pool)
            .await?;
    }
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}
