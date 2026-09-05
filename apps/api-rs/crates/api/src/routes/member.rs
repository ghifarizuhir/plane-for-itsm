use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

/// Mirrors `plane/api/serializers/member.py:ProjectMemberSerializer`
/// served by `plane/api/urls/member.py`
/// (ProjectMemberListCreateAPIEndpoint list/create): `member` must be a
/// workspace member ("Member not found in workspace"), `role` must be
/// 20/15/5 ("Invalid role"). Deactivate-on-delete belongs to detail task.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateMember {
    #[serde(default)]
    pub member: Option<uuid::Uuid>,
    #[serde(default)]
    pub role: Option<i16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemberOut {
    pub id: uuid::Uuid,
    pub member: Option<uuid::Uuid>,
    pub role: i16,
}

/// Mirrors `plane/api/serializers/invite.py:WorkspaceInviteSerializer`
/// served by `plane/api/urls/invite.py` (WorkspaceInvitationsViewset
/// list/create): email must be a valid address, role must be valid.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateInvite {
    pub email: String,
    #[serde(default)]
    pub role: Option<i16>,
}

pub const ROLES: [i16; 3] = [20, 15, 5];

pub fn validate_create(body: &CreateMember) -> Result<(), String> {
    match body.member {
        Some(_) => {}
        None => return Err("Member is required".to_string()),
    }
    let role = body.role.unwrap_or(5);
    if !ROLES.contains(&role) {
        return Err("Invalid role".to_string());
    }
    Ok(())
}

pub fn is_valid_email(email: &str) -> bool {
    let mut parts = email.split('@');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(local), Some(domain), None) => !local.is_empty() && domain.contains('.') && !domain.starts_with('.'),
        _ => false,
    }
}

pub fn validate_invite_create(body: &CreateInvite) -> Result<(), String> {
    if !is_valid_email(&body.email) {
        return Err("Invalid email address".to_string());
    }
    let role = body.role.unwrap_or(5);
    if !ROLES.contains(&role) {
        return Err("Invalid role".to_string());
    }
    Ok(())
}

pub async fn list(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, _project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<MemberOut>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::member::ProjectMember>(
        "SELECT id, member_id, role FROM project_members WHERE project_id = $1 AND deleted_at IS NULL ORDER BY created_at DESC",
    )
    .bind(_project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|m| MemberOut { id: m.id, member: m.member_id, role: m.role })
            .collect(),
    ))
}

pub async fn list_lite(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, _project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<Value>>, common::errors::AppError> {
    let ids: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT member_id FROM project_members WHERE project_id = $1 AND member_id IS NOT NULL AND deleted_at IS NULL",
    )
    .bind(_project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(ids.into_iter().map(|id| json!({"id": id})).collect()))
}

pub async fn create(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateMember>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    let member_id = body.member.unwrap();
    let role = body.role.unwrap_or(5);

    // Must already be a workspace member (serializer validate_member).
    let in_workspace: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspace_members wm JOIN workspaces w ON w.id = wm.workspace_id WHERE w.slug = $1 AND wm.member_id = $2 AND wm.deleted_at IS NULL)",
    )
    .bind(&slug)
    .bind(member_id)
    .fetch_one(&st.pool)
    .await?;
    if !in_workspace {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Member not found in workspace"}))));
    }

    let row = sqlx::query_as::<_, common::models::member::ProjectMember>(
        "INSERT INTO project_members (id, member_id, role, project_id, workspace_id, is_active, view_props, default_props, sort_order, preferences, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, $3, w.id, true, '{}', '{}', 65535, '{}', now(), now() FROM workspaces w WHERE w.slug = $4 RETURNING id, member_id, role",
    )
    .bind(member_id)
    .bind(role)
    .bind(project_id)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"id": row.id, "member": row.member_id, "role": row.role})),
    ))
}

pub async fn list_workspace_members(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<Json<Vec<MemberOut>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::member::ProjectMember>(
        "SELECT wm.id, wm.member_id, wm.role FROM workspace_members wm JOIN workspaces w ON w.id = wm.workspace_id WHERE w.slug = $1 AND wm.deleted_at IS NULL ORDER BY wm.created_at DESC",
    )
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|m| MemberOut { id: m.id, member: m.member_id, role: m.role })
            .collect(),
    ))
}

pub async fn list_invites(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<Json<Vec<Value>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::member::WorkspaceInvite>(
        "SELECT i.id, i.email, i.role FROM workspace_member_invites i JOIN workspaces w ON w.id = i.workspace_id WHERE w.slug = $1 AND i.deleted_at IS NULL ORDER BY i.created_at DESC",
    )
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|i| json!({"id": i.id, "email": i.email, "role": i.role}))
            .collect(),
    ))
}

pub async fn create_invite(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Json(body): Json<CreateInvite>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_invite_create(&body).map_err(|e| anyhow::anyhow!(e))?;
    let role = body.role.unwrap_or(5);

    let existing: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT i.id FROM workspace_member_invites i JOIN workspaces w ON w.id = i.workspace_id WHERE w.slug = $1 AND i.email = $2 AND i.deleted_at IS NULL",
    )
    .bind(&slug)
    .bind(&body.email)
    .fetch_optional(&st.pool)
    .await?;
    if let Some(id) = existing {
        return Ok((
            StatusCode::CONFLICT,
            Json(json!({"error": "Invite already exists for this email", "id": id})),
        ));
    }

    let row = sqlx::query_as::<_, common::models::member::WorkspaceInvite>(
        "INSERT INTO workspace_member_invites (id, email, role, token, accepted, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, gen_random_uuid()::text, false, w.id, now(), now() FROM workspaces w WHERE w.slug = $3 RETURNING id, email, role",
    )
    .bind(&body.email)
    .bind(role)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"id": row.id, "email": row.email, "role": row.role})),
    ))
}

/// Mirrors `plane/app/views/project/member.py:partial_update`: a non-admin
/// cannot change their own project role.
pub fn guard_patch_self(is_self: bool, is_admin: bool) -> Result<(), String> {
    if is_self && !is_admin {
        return Err("You cannot update your own role".to_string());
    }
    Ok(())
}

/// Mirrors `destroy`: self-removal must go through leave-workspace.
pub fn guard_destroy_self(is_self: bool) -> Result<(), String> {
    if is_self {
        return Err("You cannot remove yourself from the workspace. Please use leave workspace".to_string());
    }
    Ok(())
}

/// Mirrors `destroy`: requester role must not be lower than the target's.
pub fn guard_destroy_role(requester_role: i16, target_role: i16) -> Result<(), String> {
    if requester_role < target_role {
        return Err("You cannot remove a user having role higher than you".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct PatchMember {
    pub role: i16,
}

pub async fn detail(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<common::models::member::ProjectMember> = sqlx::query_as(
        "SELECT id, member_id, role FROM project_members WHERE id = $1 AND project_id = $2 AND is_active = true",
    )
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(m) => Ok((StatusCode::OK, Json(json!({"id": m.id, "role": m.role})))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Project member not found"})))),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchMember>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !ROLES.contains(&body.role) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid role"}))));
    }
    let target: Option<common::models::member::ProjectMember> = sqlx::query_as(
        "SELECT id, member_id, role FROM project_members WHERE id = $1 AND project_id = $2 AND is_active = true",
    )
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some(target) = target else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Project member not found"}))));
    };
    let requester: Option<common::models::member::ProjectMember> = sqlx::query_as(
        "SELECT id, member_id, role FROM project_members WHERE project_id = $1 AND member_id = $2 AND is_active = true",
    )
    .bind(project_id)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await?;
    let is_self = requester.as_ref().map(|r| r.id == target.id).unwrap_or(false);
    // Project-admin stands in for the workspace-admin bypass.
    let is_admin = requester.as_ref().map(|r| r.role == 20).unwrap_or(false);
    if let Err(e) = guard_patch_self(is_self, is_admin) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }
    sqlx::query("UPDATE project_members SET role = $1, updated_at = now() WHERE id = $2")
        .bind(body.role)
        .bind(pk)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::OK, Json(json!({"id": pk, "role": body.role}))))
}

pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let target: Option<common::models::member::ProjectMember> = sqlx::query_as(
        "SELECT id, member_id, role FROM project_members WHERE id = $1 AND project_id = $2 AND is_active = true",
    )
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some(target) = target else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Project member not found"}))));
    };
    let requester: Option<common::models::member::ProjectMember> = sqlx::query_as(
        "SELECT id, member_id, role FROM project_members WHERE project_id = $1 AND member_id = $2 AND is_active = true",
    )
    .bind(project_id)
    .bind(auth.0)
    .fetch_optional(&st.pool)
    .await?;
    let is_self = requester.as_ref().map(|r| r.id == target.id).unwrap_or(false);
    if let Err(e) = guard_destroy_self(is_self) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }
    if let Some(r) = &requester {
        if let Err(e) = guard_destroy_role(r.role, target.role) {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
        }
    }
    sqlx::query("UPDATE project_members SET is_active = false, updated_at = now() WHERE id = $1")
        .bind(pk)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}
