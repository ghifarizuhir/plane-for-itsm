use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::{
    middleware::auth::AuthUser,
    routes::project::{deny, project_role},
    state::AppState,
};

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
        "INSERT INTO states (id, name, description, \"group\", color, project_id, workspace_id, slug, sequence, \"default\", is_triage, created_at, updated_at) SELECT gen_random_uuid(), $1, '', $2, $3, p.id, p.workspace_id, lower(regexp_replace($1, '[^a-zA-Z0-9]+', '-', 'g')), COALESCE((SELECT MAX(sequence) FROM states WHERE project_id = p.id), 0) + 15000, false, false, now(), now() FROM projects p WHERE p.id = $4 RETURNING id, name, \"group\"",
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

/// Mirrors `plane/app/views/state/base.py:destroy`: default states and
/// non-empty states cannot be deleted.
pub fn guard_delete(is_default: bool, issue_count: i64) -> Result<(), String> {
    if is_default {
        return Err("Default state cannot be deleted".to_string());
    }
    if issue_count > 0 {
        return Err("The state is not empty, only empty states can be deleted".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchState {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

pub async fn detail(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    let row: Option<common::models::state::State> = sqlx::query_as(
        "SELECT id, name, \"group\" FROM states WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(s) => Ok((
            StatusCode::OK,
            Json(serde_json::json!({"id": s.id, "name": s.name, "group": s.group})),
        )),
        None => Ok((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "State not found"})))),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchState>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    if let Some(name) = &body.name {
        if name.trim().is_empty() || name.chars().count() > 255 {
            return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid name"}))));
        }
    }
    if let Some(group) = &body.group {
        if group == "triage" || !ALLOWED_GROUPS.contains(&group.as_str()) {
            return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid group"}))));
        }
    }
    let n = sqlx::query(
        "UPDATE states SET name = COALESCE($1, name), \"group\" = COALESCE($2, \"group\"), color = COALESCE($3, color), updated_at = now() WHERE id = $4 AND project_id = $5 AND deleted_at IS NULL",
    )
    .bind(&body.name)
    .bind(&body.group)
    .bind(&body.color)
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "State not found"}))));
    }
    Ok((StatusCode::OK, Json(serde_json::json!({"id": pk}))))
}

pub async fn destroy(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    let row: Option<(bool,)> = sqlx::query_as(
        "SELECT \"default\" FROM states WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some((is_default,)) = row else {
        return Ok((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "State not found"}))));
    };
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM issues WHERE state_id = $1 AND deleted_at IS NULL")
        .bind(pk)
        .fetch_one(&st.pool)
        .await?;
    if let Err(e) = guard_delete(is_default, count.0) {
        return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))));
    }
    sqlx::query("UPDATE states SET deleted_at = now() WHERE id = $1")
        .bind(pk)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(serde_json::json!(null))))
}

/// Mirrors `plane/app/views/state/base.py:104-106` (`mark_as_default`):
/// `@allow_permission([ROLE.ADMIN])` (level PROJECT) — only project ADMIN
/// (20) passes; MEMBER (15) / GUEST (5) / non-member → 403 via `deny()`.
/// (`ROLE` values from `plane/app/permissions/base.py:13-16`.)
pub fn guard_mark_default(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) => Ok(()),
        _ => Err(crate::routes::project::FORBIDDEN_MSG.to_string()),
    }
}

/// POST `/api/workspaces/:slug/projects/:project_id/states/:pk/mark-default/`
/// — parity with Django `StateViewSet.mark_as_default`
/// (`plane/app/views/state/base.py:104-110`).
///
/// - Gate: PROJECT ADMIN (20) only via `project_role` + `guard_mark_default`;
///   MEMBER (15) / GUEST (5) / non-member → 403 `deny()`.
/// - Two blind updates, both scoped to (workspace slug + project_id), in a
///   single tx: clear `"default"` where true, then set `"default"=true`
///   where pk (`base.py:108-109`).
/// - **204 ALWAYS** — even when pk matches 0 rows (no existence check;
///   `base.py:108-109` are blind `.update()` calls whose row counts are
///   discarded).
///
/// Queryset filters mirror Django exactly: `State.objects.filter(...)`
/// uses the default `StateManager` (`plane/db/models/state.py:65-69`),
/// which is `SoftDeletionManager` (`plane/db/mixins.py:56-58`,
/// `deleted_at IS NULL`) + `.exclude(group="triage")`. So BOTH updates
/// carry `AND deleted_at IS NULL AND "group" != 'triage'` — triage states
/// are skipped on clear AND set (a triage pk yields 204 with no change).
/// `updated_at` is NOT bumped: Django `QuerySet.update()` bypasses
/// `save()`/`auto_now`, unlike the `patch`/`archive` paths.
///
/// Deviations: single explicit transaction (Django runs two autocommit
/// UPDATEs); no cache invalidation — Rust has no cache layer (Django
/// `@invalidate_cache(path="workspaces/:slug/states/")`, `base.py:104`).
pub async fn mark_default(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, pk)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<serde_json::Value>), common::errors::AppError> {
    let role = project_role(&st.pool, auth.0, project_id).await?;
    if guard_mark_default(role).is_err() {
        return Ok(deny());
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE states SET \"default\" = false WHERE project_id = $1 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $2) AND \"default\" = true AND deleted_at IS NULL AND \"group\" != 'triage'",
    )
    .bind(project_id)
    .bind(&slug)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE states SET \"default\" = true WHERE id = $1 AND project_id = $2 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $3) AND deleted_at IS NULL AND \"group\" != 'triage'",
    )
    .bind(pk)
    .bind(project_id)
    .bind(&slug)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::NO_CONTENT, Json(serde_json::json!(null))))
}

#[cfg(test)]
mod state_mark_default_tests {
    use super::*;

    #[test]
    fn mark_default_guard_allows_admin_only() {
        // Mirrors `plane/app/views/state/base.py:104-110` (`mark_as_default`):
        // `@allow_permission([ROLE.ADMIN])` (level PROJECT) — only project
        // ADMIN (20) passes; MEMBER (15) / GUEST (5) / non-member → 403.
        assert!(guard_mark_default(Some(20)).is_ok());
        assert!(guard_mark_default(Some(15)).is_err());
        assert!(guard_mark_default(Some(5)).is_err());
        assert!(guard_mark_default(None).is_err());
    }
}
