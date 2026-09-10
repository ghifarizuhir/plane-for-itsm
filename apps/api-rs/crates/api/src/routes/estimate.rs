use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

use super::issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows};
use crate::routes::project::{deny, missing, FORBIDDEN_MSG};

/// Mirrors `plane/app/serializers/estimate.py:EstimateSerializer` /
/// `plane/api/serializers/estimate.py` served by both
/// `plane/app/urls/estimate.py` (BulkEstimatePointEndpoint list/create)
/// and `plane/api/urls/estimate.py` (ProjectEstimateAPIEndpoint).
/// Unique (name, project) → 409 mirrors the constraint
/// `estimate_unique_name_project_when_deleted_at_null`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateEstimate {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "type")]
    pub estimate_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EstimateOut {
    pub id: uuid::Uuid,
    pub name: String,
}

/// Mirrors `EstimatePointSerializer.validate`:
/// empty payload rejected, value max 20 chars; create additionally
/// requires key+value ("Key and value are required" in
/// `plane/app/views/estimate/base.py:159`).
#[derive(Debug, Clone, Deserialize)]
pub struct CreateEstimatePoint {
    #[serde(default)]
    pub key: Option<i32>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

pub fn validate_create(body: &CreateEstimate) -> Result<(), String> {
    if body.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    if body.name.chars().count() > 255 {
        return Err("name max length 255".to_string());
    }
    if let Some(t) = &body.estimate_type {
        if t != "categories" && t != "points" {
            return Err("type must be one of: categories, points".to_string());
        }
    }
    Ok(())
}

/// Mirrors `plane/app/views/estimate/base.py:partial_update`: an estimate
/// patch without points is rejected with 400.
pub fn guard_patch(points_empty: bool) -> Result<(), String> {
    if points_empty {
        return Err("Estimate points are required".to_string());
    }
    Ok(())
}

pub fn validate_point_create(body: &CreateEstimatePoint) -> Result<(), String> {
    match (&body.key, &body.value) {
        (Some(_), Some(v)) if !v.trim().is_empty() => {}
        _ => return Err("Key and value are required".to_string()),
    }
    if let Some(v) = &body.value {
        if v.chars().count() > 20 {
            return Err("Value can't be more than 20 characters".to_string());
        }
    }
    Ok(())
}

pub async fn list(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, _project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<Json<Vec<EstimateOut>>, common::errors::AppError> {
    let rows = sqlx::query_as::<_, common::models::estimate::Estimate>(
        "SELECT id, name FROM estimates WHERE project_id = $1 AND deleted_at IS NULL ORDER BY name",
    )
    .bind(_project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|e| EstimateOut { id: e.id, name: e.name })
            .collect(),
    ))
}

pub async fn create(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<CreateEstimate>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_create(&body).map_err(|e| anyhow::anyhow!(e))?;

    let existing = sqlx::query_as::<_, common::models::estimate::Estimate>(
        "SELECT id, name FROM estimates WHERE project_id = $1 AND name = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(&body.name)
    .fetch_optional(&st.pool)
    .await?;
    if let Some(estimate) = existing {
        return Ok((
            StatusCode::CONFLICT,
            Json(json!({"error": "Estimate with the same name already exists in the project", "id": estimate.id})),
        ));
    }

    let estimate_type = body.estimate_type.as_deref().unwrap_or("categories");
    let row = sqlx::query_as::<_, common::models::estimate::Estimate>(
        "INSERT INTO estimates (id, name, description, type, last_used, project_id, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, $3, false, $4, w.id, now(), now() FROM workspaces w WHERE w.slug = $5 RETURNING id, name",
    )
    .bind(&body.name)
    .bind(body.description.clone().unwrap_or_default())
    .bind(estimate_type)
    .bind(project_id)
    .bind(&slug)
    .fetch_one(&st.pool)
    .await?;
    // Django `create` (`base.py:101`) returns 200 (not 201).
    Ok((StatusCode::OK, Json(json!({"id": row.id, "name": row.name}))))
}

pub async fn create_point(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, _project_id, estimate_id)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<CreateEstimatePoint>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    validate_point_create(&body).map_err(|e| anyhow::anyhow!(e))?;

    let estimate = sqlx::query_as::<_, common::models::estimate::Estimate>(
        "SELECT id, name FROM estimates WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(estimate_id)
    .fetch_optional(&st.pool)
    .await?;
    if estimate.is_none() {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Estimate not found"}))));
    }

    let row = sqlx::query_as::<_, common::models::estimate::EstimatePoint>(
        "INSERT INTO estimate_points (id, estimate_id, key, value, description, project_id, workspace_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, $3, $4, e.project_id, e.workspace_id, now(), now() FROM estimates e WHERE e.id = $1 RETURNING id, key, value",
    )
    .bind(estimate_id)
    .bind(body.key)
    .bind(&body.value)
    .bind(body.description.clone().unwrap_or_default())
    .fetch_one(&st.pool)
    .await?;
    // Django `create` (`base.py:178-179`) returns 200 (not 201).
    Ok((
        StatusCode::OK,
        Json(json!({"id": row.id, "key": row.key, "value": row.value})),
    ))
}

#[derive(Debug, Clone, Deserialize)]
pub struct PatchEstimatePoint {
    pub id: uuid::Uuid,
    #[serde(default)]
    pub key: Option<i32>,
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchEstimate {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, rename = "type")]
    pub estimate_type: Option<String>,
    #[serde(default)]
    pub estimate_points: Vec<PatchEstimatePoint>,
}

pub async fn detail(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, estimate_id)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let row: Option<common::models::estimate::Estimate> = sqlx::query_as(
        "SELECT id, name FROM estimates WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(estimate_id)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(e) => Ok((StatusCode::OK, Json(json!({"id": e.id, "name": e.name})))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Estimate not found"})))),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, estimate_id)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchEstimate>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if let Err(e) = guard_patch(body.estimate_points.is_empty()) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }
    let exists: Option<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT id FROM estimates WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(estimate_id)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    if exists.is_none() {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Estimate not found"}))));
    }
    if let Some(name) = &body.name {
        if name.trim().is_empty() || name.chars().count() > 255 {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid name"}))));
        }
    }
    if let Some(t) = &body.estimate_type {
        if t != "categories" && t != "points" {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid estimate type"}))));
        }
    }
    sqlx::query(
        "UPDATE estimates SET name = COALESCE($1, name), type = COALESCE($2, type), updated_at = now() WHERE id = $3",
    )
    .bind(&body.name)
    .bind(&body.estimate_type)
    .bind(estimate_id)
    .execute(&st.pool)
    .await?;
    for point in &body.estimate_points {
        if let Some(v) = &point.value {
            if v.chars().count() > 20 {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "Value can't be more than 20 characters"})),
                ));
            }
        }
        sqlx::query(
            "UPDATE estimate_points SET key = COALESCE($1, key), value = COALESCE($2, value), updated_at = now() WHERE id = $3 AND estimate_id = $4",
        )
        .bind(point.key)
        .bind(&point.value)
        .bind(point.id)
        .bind(estimate_id)
        .execute(&st.pool)
        .await?;
    }
    Ok((StatusCode::OK, Json(json!({"id": estimate_id}))))
}

pub async fn destroy(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, estimate_id)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `.get` (`base.py:148`) miss → 404 (generic via `views/base.py:92-96`
    // or sane-mapped) with NO side effects. Check existence FIRST: the old order
    // deleted points before the estimate check, so a miss still wiped points.
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM estimates WHERE id = $1 AND project_id = $2)")
        .bind(estimate_id)
        .bind(project_id)
        .fetch_one(&st.pool)
        .await?;
    if !exists {
        return Ok(missing());
    }
    sqlx::query("DELETE FROM estimate_points WHERE estimate_id = $1")
        .bind(estimate_id)
        .execute(&st.pool)
        .await?;
    sqlx::query("DELETE FROM estimates WHERE id = $1 AND project_id = $2")
        .bind(estimate_id)
        .bind(project_id)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

pub async fn patch_point(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, project_id, estimate_id, point_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
        uuid::Uuid,
    )>,
    Json(body): Json<CreateEstimatePoint>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if let Some(v) = &body.value {
        if v.chars().count() > 20 {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Value can't be more than 20 characters"})),
            ));
        }
    }
    let n = sqlx::query(
        "UPDATE estimate_points SET key = COALESCE($1, key), value = COALESCE($2, value), description = COALESCE($3, description), updated_at = now() WHERE id = $4 AND estimate_id = $5 AND project_id = $6",
    )
    .bind(body.key)
    .bind(&body.value)
    .bind(&body.description)
    .bind(point_id)
    .bind(estimate_id)
    .bind(project_id)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Estimate point not found"}))));
    }
    Ok((StatusCode::OK, Json(json!({"id": point_id}))))
}

pub async fn destroy_point(
    State(st): State<AppState>,
    _auth: AuthUser,
    axum::extract::Path((_slug, _project_id, estimate_id, point_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
        uuid::Uuid,
    )>,
    Json(body): Json<serde_json::Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Optional remap: issues pointing at the deleted point move to
    // `new_estimate_id`, else their estimate is cleared — mirrors
    // `plane/app/views/estimate/base.py:destroy`.
    //
    // Django checks point existence AFTER the issue remap (`base.py:242-252`)
    // and misses with 404 `{"error": "Estimate point not found"}`. Rust checks
    // FIRST so a miss has no destructive side effects (sane-mapping precedent);
    // the miss string itself is Django-verbatim.
    let old_key: Option<i32> = sqlx::query_scalar("SELECT key FROM estimate_points WHERE id = $1 AND estimate_id = $2")
        .bind(point_id)
        .bind(estimate_id)
        .fetch_optional(&st.pool)
        .await?;
    let Some(old_key) = old_key else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Estimate point not found"}))));
    };
    let new_point = body.get("new_estimate_id").and_then(|v| v.as_str()).and_then(
        |s| uuid::Uuid::parse_str(s).ok(),
    );
    sqlx::query("UPDATE issues SET estimate_point_id = $1 WHERE estimate_point_id = $2")
        .bind(new_point)
        .bind(point_id)
        .execute(&st.pool)
        .await?;
    // Key rearrange (`base.py:254-261`): every sibling above the deleted key
    // shifts down one. Django `bulk_update`s then returns the updated set.
    sqlx::query("UPDATE estimate_points SET key = key - 1, updated_at = now() WHERE estimate_id = $1 AND key > $2")
        .bind(estimate_id)
        .bind(old_key)
        .execute(&st.pool)
        .await?;
    sqlx::query("DELETE FROM estimate_points WHERE id = $1 AND estimate_id = $2")
        .bind(point_id)
        .bind(estimate_id)
        .execute(&st.pool)
        .await?;
    // Django `destroy` (`base.py:265-268`) returns 200 with the updated-points
    // array (minimal {id,key,value} rows; full serializer remains T1).
    let rows: Vec<common::models::estimate::EstimatePoint> = sqlx::query_as(
        "SELECT id, key, value FROM estimate_points WHERE estimate_id = $1 AND key >= $2 ORDER BY key ASC",
    )
    .bind(estimate_id)
    .bind(old_key)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(json!(rows.iter().map(|r| json!({"id": r.id, "key": r.key, "value": r.value})).collect::<Vec<_>>())),
    ))
}

/// Empty-body helper for `project_estimates` below, mirroring
/// `ProjectEstimatePointEndpoint.get`
/// (`plane/app/views/estimate/base.py:34-46`): a project whose
/// `estimate_id IS NULL` responds 200 with an empty JSON array.
pub(crate) fn project_estimates_shape(estimate_id: Option<uuid::Uuid>) -> String {
    match estimate_id {
        // `estimate/base.py:46`: `return Response([], ...)`.
        None => "[]".to_string(),
        // `estimate/base.py:38-45`: rows serialized from the DB by the
        // handler — never rendered through this helper.
        Some(_) => String::new(),
    }
}

/// PROJECT-level role check: mirrors `@allow_permission([ROLE.ADMIN,
/// ROLE.MEMBER])` (`estimate/base.py:35`, default `level="PROJECT"` —
/// `permissions/base.py:17`): roles 20/15 pass; anything else (incl.
/// GUEST 5 and non-member) falls to the workspace-ADMIN fallback applied
/// by the caller via the shared `project_gate_allows` (same shape as D5
/// `guard_issue_dates`).
pub(crate) fn guard_project_estimates(role: Option<i16>) -> Result<(), String> {
    match role {
        Some(20) | Some(15) => Ok(()),
        _ => Err(FORBIDDEN_MSG.to_string()),
    }
}

/// One row of the `project-estimates/` response. Field order mirrors the
/// contract `EstimatePointSerializer.__all__` key order
/// (`plane/app/serializers/estimate.py:20-32`, columns verified against
/// `apps/api-rs/migrations/0001_initial.sql` `estimate_points` DDL);
/// struct serialization preserves declaration order. FK ids alias to
/// serializer names (`estimate_id AS estimate`, … — `versions.rs`
/// `LIST_COLUMNS` precedent). `deleted_at` is excluded per the contract;
/// live rows only (`deleted_at IS NULL`, default-manager scope).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ProjectEstimatePointRow {
    pub id: uuid::Uuid,
    pub estimate: uuid::Uuid,
    pub workspace: uuid::Uuid,
    pub project: uuid::Uuid,
    pub key: i32,
    pub value: String,
    pub description: String,
    pub created_by: Option<uuid::Uuid>,
    pub updated_by: Option<uuid::Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// GET `/api/workspaces/:slug/projects/:project_id/project-estimates/` —
/// parity with `ProjectEstimatePointEndpoint.get`
/// (`plane/app/views/estimate/base.py:34-46`,
/// `plane/app/urls/estimate.py:17-18`): project lookup by
/// `workspace__slug + pk`; `estimate_id IS NULL` → 200 `[]`, else 200
/// `EstimatePointSerializer` array filtered
/// `estimate_id + project_id + workspace(slug)` (`ORDER BY value`,
/// `EstimatePoint.Meta.ordering`).
/// Gate ADMIN/MEMBER + ws-admin fallback via shared `project_gate_allows`
/// (same shape as D5 `guard_issue_dates`).
/// Sane mapping (documented): Django `.get()` on a missing project raises
/// → 500; Rust returns 404 `missing()` (`project.rs` precedent).
pub async fn project_estimates(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let member_role = fetch_project_member_role(&st.pool, auth.0, &slug, project_id).await?;
    let ws_admin = is_workspace_admin(&st.pool, auth.0, &slug).await?;
    if !project_gate_allows(
        guard_project_estimates(member_role).is_ok(),
        member_role.is_some(),
        ws_admin,
    ) {
        return Ok(deny());
    }
    let row: Option<(Option<uuid::Uuid>,)> = sqlx::query_as(
        "SELECT p.estimate_id FROM projects p JOIN workspaces w ON w.id = p.workspace_id WHERE p.id = $1 AND w.slug = $2 AND p.deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    let Some((estimate_id,)) = row else {
        return Ok(missing());
    };
    // Django `estimate/base.py:38-46`: NULL estimate → 200 `[]`.
    let Some(estimate_id) = estimate_id else {
        let body: Value =
            serde_json::from_str(&project_estimates_shape(None)).unwrap_or(Value::Null);
        return Ok((StatusCode::OK, Json(body)));
    };
    let rows: Vec<ProjectEstimatePointRow> = sqlx::query_as(
        "SELECT ep.id, ep.estimate_id AS estimate, ep.workspace_id AS workspace, \
         ep.project_id AS project, ep.key, ep.value, ep.description, \
         ep.created_by_id AS created_by, ep.updated_by_id AS updated_by, \
         ep.created_at, ep.updated_at FROM estimate_points ep \
         JOIN workspaces w ON w.id = ep.workspace_id \
         WHERE ep.estimate_id = $1 AND ep.project_id = $2 AND w.slug = $3 \
         AND ep.deleted_at IS NULL ORDER BY ep.value ASC",
    )
    .bind(estimate_id)
    .bind(project_id)
    .bind(&slug)
    .fetch_all(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(json!(rows))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_estimates_no_estimate_returns_empty_list() {
        // Django returns `[]` when project.estimate_id is null (estimate/base.py:38-45).
        assert_eq!(project_estimates_shape(None).as_str(), "[]");
    }
}
