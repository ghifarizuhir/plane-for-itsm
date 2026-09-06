use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::routes::project::{deny, project_role, ws_role, FORBIDDEN_MSG};
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

/// Mirrors `plane/app/views/view/base.py:retrieve`: a project guest sees a
/// view only when guests may view all features or they own it.
pub fn guard_guest_access(is_guest: bool, guest_view_all: bool, is_owner: bool) -> Result<(), String> {
    if is_guest && !guest_view_all && !is_owner {
        return Err("You are not allowed to view this issue".to_string());
    }
    Ok(())
}

// Kept for `tests/view_test.rs` per user (E6): the favorite handler
// intentionally skips validation (NULL passthrough), so routes never call
// this — but the integration test still exercises it.
#[allow(dead_code)]
pub fn validate_favorite_create(body: &CreateFavorite) -> Result<(), String> {
    if body.view.is_none() {
        return Err("view is required".to_string());
    }
    Ok(())
}

/// `plane/app/views/base.py:92-97` (Django `IntegrityError` → 400; favorite dup).
pub const PAYLOAD_INVALID_MSG: &str = "The payload is not valid";

/// Mirrors `@allow_permission([ROLE.ADMIN, ROLE.MEMBER])`
/// (`plane/app/permissions/base.py:40-59`) — GUEST (5) denied.
pub fn guard_am(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// Gate for the view-favorite AM endpoint: allowed project role 20/15
/// outright, else the workspace-ADMIN fallback
/// (`plane/app/permissions/base.py:61-78` — any active project membership +
/// workspace ADMIN), mirroring `cycle.rs:gate_am`. Anything else denies.
async fn gate_am(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let role = project_role(pool, user, pid).await?;
    let ws_admin = ws_role(pool, user, slug)
        .await
        .map(|r| r == Some(20))
        .unwrap_or(false);
    Ok(guard_am(role).is_ok() || (role.is_some() && ws_admin))
}

/// Pure SQLSTATE check behind [`is_constraint_violation`]: class `23`
/// (integrity constraint violation — Django `IntegrityError` → 400).
/// Split out so unit tests can exercise the dup-arm mapping without
/// constructing a `sqlx::Error` (which has no public test constructor).
fn is_constraint_violation_code(code: &str) -> bool {
    code.starts_with("23")
}

fn is_constraint_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().is_some_and(|c| is_constraint_violation_code(&c)))
}

/// Pure dup→400 mapping for the favorite insert (`views/base.py:92-97`):
/// `true` (constraint violation) → 400 `PAYLOAD_INVALID_MSG`,
/// `false` (insert ok) → 204. The handler delegates here so unit tests
/// fail if the dup-arm mapping breaks.
fn favorite_insert_outcome(is_dup: bool) -> (StatusCode, Json<Value>) {
    if is_dup {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": PAYLOAD_INVALID_MSG})),
        )
    } else {
        (StatusCode::NO_CONTENT, Json(Value::Null))
    }
}

/// Pure extraction of the favorite target (`view/base.py:420-427`):
/// `request.data.get("view")` passes None straight through
/// (`entity_identifier` is nullable) — no validation, NULL still 204s.
fn favorite_entity_identifier(body: &CreateFavorite) -> Option<uuid::Uuid> {
    body.view
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
    let owner = auth.0;
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
    let owner = auth.0;
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
    // `@allow_permission([ROLE.ADMIN, ROLE.MEMBER])`
    // (`plane/app/views/view/base.py:419`): project ADMIN (20) / MEMBER (15)
    // only; GUEST (5) / non-member → 403 `deny()`
    // (`plane/app/permissions/base.py:81-84`).
    if !gate_am(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    // `plane/app/views/view/base.py:420-427`: NO body validation —
    // `request.data.get("view")` passes None straight through
    // (`user_favorites.entity_identifier` is nullable) and still 204s.
    let view_id: Option<uuid::Uuid> = favorite_entity_identifier(&body);
    let owner = auth.0;

    let r = sqlx::query(
        "INSERT INTO user_favorites (id, entity_type, entity_identifier, user_id, is_folder, sequence, project_id, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), 'view', $1, $2, false, 65535, $3, w.id, now(), now() FROM workspaces w WHERE w.slug = $4",
    )
    .bind(view_id)
    .bind(owner)
    .bind(project_id)
    .bind(&slug)
    .execute(&st.pool)
    .await;
    // Dup → 400 `{"error": "The payload is not valid"}` (Django
    // `IntegrityError` → `plane/app/views/base.py:92-97`); unique index
    // `(entity_type, entity_identifier, user_id) WHERE deleted_at IS NULL`.
    match r {
        Ok(_) => Ok(favorite_insert_outcome(false)),
        Err(e) if is_constraint_violation(&e) => Ok(favorite_insert_outcome(true)),
        Err(e) => Err(e.into()),
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchView {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub access: Option<i16>,
}

async fn view_detail_row(
    st: &AppState,
    auth: &AuthUser,
    slug: &str,
    project_id: Option<uuid::Uuid>,
    pk: uuid::Uuid,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<(uuid::Uuid, String, Option<uuid::Uuid>)> = if let Some(pid) = project_id {
        sqlx::query_as(
            "SELECT v.id, v.name, v.owned_by_id FROM views v WHERE v.id = $1 AND v.project_id = $2 AND v.deleted_at IS NULL",
        )
        .bind(pk)
        .bind(pid)
        .fetch_optional(&st.pool)
        .await?
    } else {
        sqlx::query_as(
            "SELECT v.id, v.name, v.owned_by_id FROM views v JOIN workspaces w ON w.id = v.workspace_id WHERE v.id = $1 AND w.slug = $2 AND v.deleted_at IS NULL",
        )
        .bind(pk)
        .bind(slug)
        .fetch_optional(&st.pool)
        .await?
    };
    let Some((id, name, owned_by)) = row else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "View not found"}))));
    };
    // Guest gate mirrors `IssueViewViewSet.retrieve`.
    // AuthUser identitas sudah tervalidasi di extractor.
    let uid = auth.0;
    if let Some(pid) = project_id {
        let role: Option<i16> = sqlx::query_scalar(
            "SELECT role FROM project_members WHERE project_id = $1 AND member_id = $2 AND is_active = true",
        )
        .bind(pid)
        .bind(uid)
        .fetch_optional(&st.pool)
        .await?;
        let gva: Option<bool> =
            sqlx::query_scalar("SELECT guest_view_all_features FROM projects WHERE id = $1")
                .bind(pid)
                .fetch_optional(&st.pool)
                .await?;
        if let Err(e) = guard_guest_access(
            role == Some(5),
            gva.unwrap_or(false),
            owned_by == Some(uid),
        ) {
            return Ok((StatusCode::FORBIDDEN, Json(json!({"error": e}))));
        }
    }
    Ok((StatusCode::OK, Json(json!({"id": id, "name": name}))))
}

pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    view_detail_row(&st, &auth, &_slug, Some(project_id), pk).await
}

pub async fn detail_global(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, pk)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    view_detail_row(&st, &auth, &slug, None, pk).await
}

pub async fn patch(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchView>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if let Some(name) = &body.name {
        if name.trim().is_empty() || name.chars().count() > 255 {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid name"}))));
        }
    }
    if let Some(access) = body.access {
        if access != 0 && access != 1 {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid access"}))));
        }
    }
    let n = sqlx::query(
        "UPDATE views SET name = COALESCE($1, name), access = COALESCE($2, access), updated_at = now() WHERE id = $3 AND project_id = $4 AND deleted_at IS NULL",
    )
    .bind(&body.name)
    .bind(body.access)
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "View not found"}))));
    }
    Ok((StatusCode::OK, Json(json!({"id": pk}))))
}

pub async fn destroy(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    sqlx::query(
        "UPDATE views SET deleted_at = now() WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

#[cfg(test)]
mod view_e6_tests {
    use super::*;
    use crate::routes::project::deny;

    #[test]
    fn fav_gate_allows_admin_member_only() {
        // `@allow_permission([ROLE.ADMIN, ROLE.MEMBER])`
        // (`permissions/base.py:40-59`): project ADMIN (20) / MEMBER (15)
        // pass; GUEST (5) / non-member → 403 `deny()`.
        assert!(guard_am(Some(20)).is_ok());
        assert!(guard_am(Some(15)).is_ok());
        assert!(guard_am(Some(5)).is_err());
        assert!(guard_am(None).is_err());
    }

    #[test]
    fn fav_dup_maps_to_400_payload_invalid() {
        // Django `IntegrityError` → `views/base.py:92-97`: a duplicate
        // favorite is 400 `{"error": "The payload is not valid"}` — never
        // the 409 `View already favorited` shape. Exercises the real
        // pure helpers the handler delegates to, so the test fails if
        // either the SQLSTATE check or the dup-arm mapping breaks.
        assert!(is_constraint_violation_code("23505"));
        assert!(!is_constraint_violation_code("22000"));

        let (status, body) = favorite_insert_outcome(true);
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body.0, json!({"error": "The payload is not valid"}));
        assert!(!PAYLOAD_INVALID_MSG.contains("favorited"));

        let (status, body) = favorite_insert_outcome(false);
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(body.0, Value::Null);
    }

    #[test]
    fn fav_deny_is_403_permissions_error() {
        // `permissions/base.py:81-84`: the AM-gate deny body.
        let (status, body) = deny();
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(
            body.0,
            json!({"error": "You don't have the required permissions."})
        );
    }

    #[test]
    fn fav_missing_view_deserializes_to_none() {
        // `view/base.py:421-424`: `request.data.get("view")` passes None
        // straight through (`entity_identifier` is nullable) — the handler
        // stores NULL and still returns 204, with no validation error.
        // Exercises the real extraction helper (not just serde), so the
        // NULL-passthrough contract breaks loudly if it changes. No DB
        // infra needed: the helper is pure.
        let body: CreateFavorite = serde_json::from_value(json!({})).unwrap();
        assert_eq!(body.view, None);
        assert_eq!(favorite_entity_identifier(&body), None);

        let id = uuid::Uuid::new_v4();
        let body: CreateFavorite =
            serde_json::from_value(json!({ "view": id })).unwrap();
        assert_eq!(body.view, Some(id));
        assert_eq!(favorite_entity_identifier(&body), Some(id));
    }
}
