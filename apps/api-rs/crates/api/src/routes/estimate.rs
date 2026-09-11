use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{middleware::auth::AuthUser, state::AppState};

use super::issue_common::{fetch_project_member_role, is_workspace_admin, project_gate_allows};
use crate::routes::project::{deny, missing, FORBIDDEN_MSG, INVALID_PAYLOAD_MSG};

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

/// Full estimate row: `Estimate.__all__` (`serializers/estimate.py:35-41`
/// via `EstimateReadSerializer`) + nested `points`. `deleted_at` omitted
/// for live rows (H2 precedent); datetimes RFC3339 UTC (batch convention).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EstimateFull {
    pub id: uuid::Uuid,
    pub name: String,
    pub description: String,
    pub estimate_type: String,
    pub last_used: bool,
    pub project_id: uuid::Uuid,
    pub workspace_id: uuid::Uuid,
    pub created_by_id: Option<uuid::Uuid>,
    pub updated_by_id: Option<uuid::Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Full point row: `EstimatePoint.__all__` (`serializers/estimate.py:20-32`)
/// with DRF relational names (`estimate`/`project`/`workspace`/
//// `created_by`/`updated_by`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EstimatePointFull {
    pub id: uuid::Uuid,
    pub key: i32,
    pub value: String,
    pub description: String,
    pub estimate_id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub workspace_id: uuid::Uuid,
    pub created_by_id: Option<uuid::Uuid>,
    pub updated_by_id: Option<uuid::Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub fn point_json(p: &EstimatePointFull) -> Value {
    json!({
        "id": p.id,
        "key": p.key,
        "value": p.value,
        "description": p.description,
        "estimate": p.estimate_id,
        "project": p.project_id,
        "workspace": p.workspace_id,
        "created_by": p.created_by_id,
        "updated_by": p.updated_by_id,
        "created_at": p.created_at,
        "updated_at": p.updated_at,
    })
}

pub fn estimate_json(e: &EstimateFull, points: Vec<Value>) -> Value {
    json!({
        "id": e.id,
        "name": e.name,
        "description": e.description,
        "type": e.estimate_type,
        "last_used": e.last_used,
        "project": e.project_id,
        "workspace": e.workspace_id,
        "created_by": e.created_by_id,
        "updated_by": e.updated_by_id,
        "created_at": e.created_at,
        "updated_at": e.updated_at,
        "points": points,
    })
}

const ESTIMATE_COLS: &str = "id, name, description, type AS estimate_type, last_used, project_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at";
const POINT_COLS: &str = "id, key, value, description, estimate_id, project_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at";

/// Points for one estimate in Django `Meta.ordering` (`value`,
/// `models/estimate.py:47`) — same order as the verified workspace
/// estimates twin.
async fn estimate_points(
    pool: &sqlx::PgPool,
    estimate_id: uuid::Uuid,
) -> Result<Vec<Value>, sqlx::Error> {
    let rows: Vec<EstimatePointFull> = sqlx::query_as(&format!(
        "SELECT {POINT_COLS} FROM estimate_points WHERE estimate_id = $1 AND deleted_at IS NULL ORDER BY value ASC"
    ))
    .bind(estimate_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(point_json).collect())
}

/// `ProjectEntityPermission` safe branch (GET): any active project member
/// passes, guests included (`permissions/project.py:88-116`), with the
/// shared ws-admin fallback.
async fn gate_estimate_read(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, pid).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(role.is_some(), role.is_some(), ws_admin))
}

/// `ProjectEntityPermission` unsafe branch (writes): ADMIN/MEMBER only.
async fn gate_estimate_write(
    pool: &sqlx::PgPool,
    user: uuid::Uuid,
    slug: &str,
    pid: uuid::Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, pid).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(matches!(role, Some(20) | Some(15)), role.is_some(), ws_admin))
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
    // Django `create` (`base.py:157-161`): `not request.data.get("key")` —
    // a missing key AND a falsy `0` both 400 (single-create only; PATCH
    // goes through the serializer, which accepts 0).
    match (&body.key, &body.value) {
        (Some(k), Some(v)) if *k != 0 && !v.trim().is_empty() => {}
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
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `list` (`base.py:54-61`): any project member reads.
    if !gate_estimate_read(&st.pool, auth.0, &slug, project_id).await? {
        return Ok((StatusCode::FORBIDDEN, Json(json!({"error": FORBIDDEN_MSG}))));
    }
    let rows: Vec<EstimateFull> = sqlx::query_as(&format!(
        "SELECT {ESTIMATE_COLS} FROM estimates WHERE project_id = $1 AND deleted_at IS NULL ORDER BY name ASC"
    ))
    .bind(project_id)
    .fetch_all(&st.pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for e in &rows {
        out.push(estimate_json(e, estimate_points(&st.pool, e.id).await?));
    }
    Ok((StatusCode::OK, Json(Value::Array(out))))
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id)): axum::extract::Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `create` (`base.py:63-101`): ADMIN/MEMBER writes.
    if !gate_estimate_write(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    // Body shapes: Django nests (`{estimate: {name, type, last_used},
    // estimate_points: [{key, value, description}]}`, `base.py:65-76`) —
    // the FE `IEstimateFormData` shape. The legacy flat Rust body
    // (`{name, description, type}`) stays accepted as a superset.
    let nested = body.get("estimate").is_some() || body.get("estimate_points").is_some();
    let estimate_node = body.get("estimate");
    let name = estimate_node
        .and_then(|e| e.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| body.get("name").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();
    // Django defaults an empty name to a random string (`base.py:66`);
    // the flat shape keeps the explicit 400 instead (documented).
    let estimate_type = estimate_node
        .and_then(|e| e.get("type"))
        .and_then(Value::as_str)
        .or_else(|| body.get("type").and_then(Value::as_str))
        .unwrap_or("categories");
    if name.trim().is_empty() && !nested {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "name is required"}))));
    }
    if estimate_type != "categories" && estimate_type != "points" {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "type must be one of: categories, points"}))));
    }
    let final_name = if name.trim().is_empty() {
        format!("estimate-{}", &uuid::Uuid::new_v4().simple().to_string()[..10])
    } else {
        name
    };
    if final_name.chars().count() > 255 {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": "name max length 255"}))));
    }
    let last_used = estimate_node.and_then(|e| e.get("last_used")).and_then(Value::as_bool).unwrap_or(false);
    // Bulk points use serializer defaults (`key=0`, `value=""`,
    // `base.py:83-96`); over-long values 400 like the single-point rule
    // (`serializers/estimate.py:20-32` validate).
    let mut points: Vec<(i32, String, String)> = Vec::new();
    if let Some(arr) = body.get("estimate_points").and_then(Value::as_array) {
        for p in arr {
            let value = p.get("value").and_then(Value::as_str).unwrap_or("").to_string();
            if value.chars().count() > 20 {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "Value can't be more than 20 characters"})),
                ));
            }
            let key = p.get("key").and_then(Value::as_i64).unwrap_or(0) as i32;
            let description = p.get("description").and_then(Value::as_str).unwrap_or("").to_string();
            points.push((key, value, description));
        }
    }
    // Serializer-first validation runs before the dup check in Django
    // (`base.py:78-82` then the DB constraint); a dup name hits the
    // partial unique constraint → `IntegrityError` → 400
    // `{"error": "The payload is not valid"}` via `views/base.py:80-84`
    // (NOT 500 — the handler maps it).
    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM estimates WHERE project_id = $1 AND name = $2 AND deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(&final_name)
    .fetch_one(&st.pool)
    .await?;
    if dup {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": INVALID_PAYLOAD_MSG})),
        ));
    }
    let workspace_id: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT id FROM workspaces WHERE slug = $1").bind(&slug).fetch_optional(&st.pool).await?;
    let Some(workspace_id) = workspace_id else {
        return Ok(missing());
    };
    let created: EstimateFull = sqlx::query_as(&format!(
        "INSERT INTO estimates (id, name, description, type, last_used, project_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $7, now(), now()) RETURNING {ESTIMATE_COLS}"
    ))
    .bind(&final_name)
    .bind(body.get("description").and_then(Value::as_str).unwrap_or(""))
    .bind(estimate_type)
    .bind(last_used)
    .bind(project_id)
    .bind(workspace_id)
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await?;
    for (key, value, description) in &points {
        sqlx::query(
            "INSERT INTO estimate_points (id, estimate_id, key, value, description, project_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $7, now(), now())",
        )
        .bind(created.id)
        .bind(key)
        .bind(value)
        .bind(description)
        .bind(project_id)
        .bind(workspace_id)
        .bind(auth.0)
        .execute(&st.pool)
        .await?;
    }
    // Django `create` (`base.py:101`) returns 200 (not 201).
    let pts = estimate_points(&st.pool, created.id).await?;
    Ok((StatusCode::OK, Json(estimate_json(&created, pts))))
}

pub async fn create_point(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, estimate_id)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<CreateEstimatePoint>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `create` (`base.py:154`): ADMIN/MEMBER (permission classes
    // first → miss+non-member is 403, not 404).
    if !gate_estimate_write(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    // Django serializer 400s (never 500s) on invalid bodies.
    if let Err(e) = validate_point_create(&body) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }

    // Django scopes the estimate to this workspace+project
    // (`base.py:163-172` `.filter(...).first()` → 404 "Estimate not found");
    // an estimate from another project must NOT accept points here.
    let estimate = sqlx::query_as::<_, common::models::estimate::Estimate>(
        "SELECT id, name FROM estimates WHERE id = $1 AND project_id = $2 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $3) AND deleted_at IS NULL",
    )
    .bind(estimate_id)
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    if estimate.is_none() {
        return Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Estimate not found"}))));
    }

    let row: EstimatePointFull = sqlx::query_as(&format!(
        "INSERT INTO estimate_points (id, estimate_id, key, value, description, project_id, workspace_id, created_by_id, updated_by_id, created_at, updated_at) SELECT gen_random_uuid(), $1, $2, $3, $4, e.project_id, e.workspace_id, $5, $5, now(), now() FROM estimates e WHERE e.id = $1 RETURNING {POINT_COLS}"
    ))
    .bind(estimate_id)
    .bind(body.key)
    .bind(&body.value)
    .bind(body.description.clone().unwrap_or_default())
    .bind(auth.0)
    .fetch_one(&st.pool)
    .await?;
    // Django `create` (`base.py:178-179`) returns 200 (not 201) full row.
    Ok((StatusCode::OK, Json(point_json(&row))))
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
    auth: AuthUser,
    axum::extract::Path((slug, project_id, estimate_id)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `retrieve` (`base.py:103-106`): `ProjectEntityPermission`
    // (permission classes run BEFORE the body, so a non-member 403s even
    // on a miss) — gate first, then the `.get()` miss → 404.
    if !gate_estimate_read(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let row: Option<EstimateFull> = sqlx::query_as(&format!(
        "SELECT {ESTIMATE_COLS} FROM estimates WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL"
    ))
    .bind(estimate_id)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(e) => {
            let pts = estimate_points(&st.pool, e.id).await?;
            Ok((StatusCode::OK, Json(estimate_json(&e, pts))))
        }
        None => Ok(missing()),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, estimate_id)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<PatchEstimate>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `partial_update` (`base.py:109`): ADMIN/MEMBER writes
    // (permission classes first → miss+non-member is 403, not 404).
    if !gate_estimate_write(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    if let Err(e) = guard_patch(body.estimate_points.is_empty()) {
        return Ok((StatusCode::BAD_REQUEST, Json(json!({"error": e}))));
    }
    let exists: Option<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT id FROM estimates WHERE id = $1 AND project_id = $2 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $3) AND deleted_at IS NULL",
    )
    .bind(estimate_id)
    .bind(project_id)
    .bind(&slug)
    .fetch_optional(&st.pool)
    .await?;
    if exists.is_none() {
        return Ok(missing());
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
    // A rename that collides hits the partial unique constraint →
    // `IntegrityError` → 400 payload (`views/base.py:80-84`), same as
    // create. Checked up front (no partial writes on dup).
    if let Some(name) = &body.name {
        let dup: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM estimates WHERE project_id = $1 AND name = $2 AND id != $3 AND deleted_at IS NULL)",
        )
        .bind(project_id)
        .bind(name)
        .bind(estimate_id)
        .fetch_one(&st.pool)
        .await?;
        if dup {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": INVALID_PAYLOAD_MSG})),
            ));
        }
    }
    sqlx::query(
        "UPDATE estimates SET name = COALESCE($1, name), type = COALESCE($2, type), updated_at = now() WHERE id = $3 AND project_id = $4 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $5)",
    )
    .bind(&body.name)
    .bind(&body.estimate_type)
    .bind(estimate_id)
    .bind(project_id)
    .bind(&slug)
    .execute(&st.pool)
    .await?;
    // Django sets point attributes directly + `bulk_update` with NO
    // per-point validation (`base.py:122-141`) — over-long values are
    // saved as-is (unlike single-create). No length check here.
    for point in &body.estimate_points {
        sqlx::query(
            "UPDATE estimate_points SET key = COALESCE($1, key), value = COALESCE($2, value), updated_at = now() WHERE id = $3 AND estimate_id = $4 AND project_id = $5 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $6)",
        )
        .bind(point.key)
        .bind(&point.value)
        .bind(point.id)
        .bind(estimate_id)
        .bind(project_id)
        .bind(&slug)
        .execute(&st.pool)
        .await?;
    }
    // Django `partial_update` (`base.py:143-144`) returns 200 full row.
    let row: Option<EstimateFull> = sqlx::query_as(&format!(
        "SELECT {ESTIMATE_COLS} FROM estimates WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL"
    ))
    .bind(estimate_id)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(e) => {
            let pts = estimate_points(&st.pool, e.id).await?;
            Ok((StatusCode::OK, Json(estimate_json(&e, pts))))
        }
        None => Ok(missing()),
    }
}

pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, estimate_id)): axum::extract::Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `destroy` (`base.py:146-150`): ADMIN/MEMBER writes
    // (permission classes first → miss+non-member is 403, not 404).
    if !gate_estimate_write(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    // Django `.get` (`base.py:148`) miss → 404 (generic via `views/base.py:92-96`
    // or sane-mapped) with NO side effects. Check existence FIRST: the old order
    // deleted points before the estimate check, so a miss still wiped points.
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM estimates WHERE id = $1 AND project_id = $2 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $3) AND deleted_at IS NULL)")
        .bind(estimate_id)
        .bind(project_id)
        .bind(&slug)
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
    // `Project.estimate` FK is `on_delete=SET_NULL`
    // (`models/project.py:109`): deleting the active estimate clears the
    // project pointer (Django does this via the collector, not code).
    sqlx::query("UPDATE projects SET estimate_id = NULL WHERE estimate_id = $1 AND id = $2")
        .bind(estimate_id)
        .bind(project_id)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(json!(null))))
}

pub async fn patch_point(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, estimate_id, point_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
        uuid::Uuid,
    )>,
    Json(body): Json<CreateEstimatePoint>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `partial_update` (`base.py:181`): ADMIN/MEMBER.
    if !gate_estimate_write(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
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
    // Django returns 200 full `EstimatePointSerializer`.
    let row: Option<EstimatePointFull> = sqlx::query_as(&format!(
        "SELECT {POINT_COLS} FROM estimate_points WHERE id = $1 AND deleted_at IS NULL"
    ))
    .bind(point_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(p) => Ok((StatusCode::OK, Json(point_json(&p)))),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({"error": "Estimate point not found"})))),
    }
}

pub async fn destroy_point(
    State(st): State<AppState>,
    auth: AuthUser,
    axum::extract::Path((slug, project_id, estimate_id, point_id)): axum::extract::Path<(
        String,
        uuid::Uuid,
        uuid::Uuid,
        uuid::Uuid,
    )>,
    Json(body): Json<serde_json::Value>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    // Django `destroy` (`base.py:196`): ADMIN/MEMBER (permission classes
    // first → miss+non-member is 403, not 404).
    if !gate_estimate_write(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    // Optional remap: issues pointing at the deleted point move to
    // `new_estimate_id`, else their estimate is cleared — mirrors
    // `plane/app/views/estimate/base.py:destroy`.
    //
    // Django checks point existence AFTER the issue remap (`base.py:242-252`)
    // and misses with 404 `{"error": "Estimate point not found"}`. Rust checks
    // FIRST so a miss has no destructive side effects (sane-mapping precedent,
    // ADR `2026-09-10-f1-group3-delete-semantics.md`); the miss string itself
    // is Django-verbatim.
    let old_key: Option<i32> = sqlx::query_scalar("SELECT key FROM estimate_points WHERE id = $1 AND estimate_id = $2 AND project_id = $3 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $4)")
        .bind(point_id)
        .bind(estimate_id)
        .bind(project_id)
        .bind(&slug)
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
    // array (the REARRANGED rows only, in `Meta.ordering` = `value` ASC),
    // full `EstimatePointSerializer` rows.
    let rows: Vec<EstimatePointFull> = sqlx::query_as(&format!(
        "SELECT {POINT_COLS} FROM estimate_points WHERE estimate_id = $1 AND key >= $2 AND deleted_at IS NULL ORDER BY value ASC"
    ))
    .bind(estimate_id)
    .bind(old_key)
    .fetch_all(&st.pool)
    .await?;
    Ok((StatusCode::OK, Json(json!(rows.iter().map(point_json).collect::<Vec<_>>()))))
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

    #[test]
    fn point_create_rejects_falsy_key_like_django() {
        // Django `create` (`base.py:157-161`): `not request.data.get("key")`
        // — missing AND `0` both 400 (single-create only).
        assert!(
            validate_point_create(&CreateEstimatePoint {
                key: Some(0),
                value: Some("x".to_string()),
                description: None,
            })
            .is_err()
        );
        assert!(
            validate_point_create(&CreateEstimatePoint {
                key: None,
                value: Some("x".to_string()),
                description: None,
            })
            .is_err()
        );
        assert!(
            validate_point_create(&CreateEstimatePoint {
                key: Some(1),
                value: Some("x".to_string()),
                description: None,
            })
            .is_ok()
        );
    }

    #[test]
    fn dup_name_maps_to_integrity_payload_like_django() {
        // Dup (name, project) hits the partial unique constraint →
        // `IntegrityError` → 400 "The payload is not valid" via
        // `views/base.py:80-84` (NOT 500, NOT 409).
        assert_eq!(INVALID_PAYLOAD_MSG, "The payload is not valid");
    }
}
