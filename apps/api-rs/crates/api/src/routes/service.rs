use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    middleware::auth::AuthUser,
    routes::project::{deny, missing},
    state::AppState,
};

use super::issue_common::{
    fetch_project_member_role, is_workspace_admin, project_gate_allows,
};
use super::service_access;

/// Allowed `status` values (`packages/types/src/service/core.ts`).
pub const SERVICE_STATUSES: &[&str] =
    &["active", "planned", "maintenance", "deprecated", "retired"];
/// Allowed `criticality` values.
pub const SERVICE_CRITICALITIES: &[&str] = &["critical", "high", "medium", "low"];
/// Allowed `type` values.
pub const SERVICE_TYPES: &[&str] = &["internal", "external", "infrastructure", "third_party"];

/// trim + lowercase, matching the mock's case-insensitive uniqueness.
pub fn normalize_name(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Enum validation; `field` appears in the error message (`"Invalid status"`).
pub fn validate_enum(field: &str, value: &str, allowed: &[&str]) -> Result<(), String> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(format!("Invalid {field}"))
    }
}

/// True when adding edge `from -> to` creates a cycle, i.e. `from` is
/// reachable from `to` following existing `(from, to)` edges. `from == to`
/// is a self-edge and also reported as a cycle.
pub fn would_create_cycle(edges: &[(Uuid, Uuid)], from: Uuid, to: Uuid) -> bool {
    if from == to {
        return true;
    }
    let mut stack = vec![to];
    let mut seen = std::collections::HashSet::new();
    while let Some(node) = stack.pop() {
        if !seen.insert(node) {
            continue;
        }
        for (a, b) in edges {
            if *a == node {
                if *b == from {
                    return true;
                }
                stack.push(*b);
            }
        }
    }
    false
}

/// Append order: `max(0, existing_max) + 65535`, matching the mock.
pub fn next_sort_order(max_existing: f64) -> f64 {
    max_existing.max(0.0) + 65535.0
}

// ---------------------------------------------------------------------------
// Row + serializer
// ---------------------------------------------------------------------------

/// Full `services` row (no `project_id`: ownership is link-derived).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ServiceRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub description: String,
    pub description_html: String,
    pub status: String,
    pub criticality: String,
    pub service_type: String,
    pub owner_id: Option<Uuid>,
    pub repository_url: Option<String>,
    pub documentation_url: Option<String>,
    pub position: Option<serde_json::Value>,
    pub sort_order: f64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by_id: Option<Uuid>,
    pub updated_by_id: Option<Uuid>,
}

const SERVICE_SELECT: &str = "SELECT s.id, s.workspace_id, s.name, s.description, \
    s.description_html, s.status, s.criticality, s.\"type\" AS service_type, s.owner_id, \
    s.repository_url, s.documentation_url, s.position, s.sort_order, s.created_at, s.updated_at, \
    s.created_by_id, s.updated_by_id FROM services s";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ServiceLinkRow {
    pub id: Uuid,
    pub service_id: Uuid,
    pub project_id: Uuid,
    pub link_type: String,
}

pub async fn links_for_services(
    pool: &sqlx::PgPool,
    ids: &[Uuid],
) -> Result<Vec<ServiceLinkRow>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    sqlx::query_as(
        "SELECT id, service_id, project_id, link_type FROM service_project_links \
         WHERE service_id = ANY($1) AND deleted_at IS NULL ORDER BY link_type, created_at",
    )
    .bind(ids)
    .fetch_all(pool)
    .await
}

pub fn owner_project_of(links: &[ServiceLinkRow]) -> Option<Uuid> {
    links
        .iter()
        .find(|l| l.link_type == "owner")
        .map(|l| l.project_id)
}

/// Full service JSON. `project_id` is retained (deprecated) = owner project id.
pub fn service_json(row: &ServiceRow, links: &[ServiceLinkRow], access_level: &str) -> Value {
    let owner_project_id = owner_project_of(links);
    let project_links: Vec<Value> = links
        .iter()
        .map(|l| {
            json!({
                "id": l.id,
                "project_id": l.project_id,
                "link_type": l.link_type,
            })
        })
        .collect();
    json!({
        "id": row.id,
        "workspace_id": row.workspace_id,
        "project_id": owner_project_id,
        "owner_project_id": owner_project_id,
        "project_links": project_links,
        "access_level": access_level,
        "name": row.name,
        "description": row.description,
        "description_html": row.description_html,
        "status": row.status,
        "criticality": row.criticality,
        "type": row.service_type,
        "owner_id": row.owner_id,
        "repository_url": row.repository_url,
        "documentation_url": row.documentation_url,
        "position": row.position,
        "sort_order": row.sort_order,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "created_by": row.created_by_id,
        "updated_by": row.updated_by_id,
    })
}

/// Lite projection used for dependency endpoints outside the caller's
/// full-visibility set (design §3).
pub fn lite_service_json(id: Uuid, name: &str, status: &str, criticality: &str) -> Value {
    json!({
        "id": id,
        "name": name,
        "status": status,
        "criticality": criticality,
        "access_level": "lite",
    })
}

pub(crate) async fn service_json_by_id(
    pool: &sqlx::PgPool,
    service_id: Uuid,
    access_level: &str,
) -> Result<Option<Value>, sqlx::Error> {
    let row: Option<ServiceRow> = sqlx::query_as(&format!("{SERVICE_SELECT} WHERE s.id = $1"))
        .bind(service_id)
        .fetch_optional(pool)
        .await?;
    match row {
        Some(row) => {
            let links = links_for_services(pool, &[row.id]).await?;
            Ok(Some(service_json(&row, &links, access_level)))
        }
        None => Ok(None),
    }
}

/// Shared list serializer: fetch links once, group per service.
pub(crate) async fn json_service_list(
    pool: &sqlx::PgPool,
    rows: &[ServiceRow],
    access_level: &str,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let links = links_for_services(pool, &ids).await?;
    let payload: Vec<Value> = rows
        .iter()
        .map(|row| {
            let row_links: Vec<ServiceLinkRow> = links
                .iter()
                .filter(|l| l.service_id == row.id)
                .cloned()
                .collect();
            service_json(row, &row_links, access_level)
        })
        .collect();
    Ok((StatusCode::OK, Json(Value::Array(payload))))
}

#[derive(Debug, Deserialize, Default)]
pub struct WorkspaceServiceListParams {
    pub project: Option<Uuid>,
    pub q: Option<String>,
    pub status: Option<String>,
    pub criticality: Option<String>,
    pub link: Option<String>,
}

pub async fn list_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(params): Query<WorkspaceServiceListParams>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let visible: Option<Vec<Uuid>> = if is_workspace_admin(&st.pool, auth.0, &slug).await? {
        None
    } else {
        Some(service_access::visible_service_ids(&st.pool, auth.0, &slug).await?)
    };
    let rows: Vec<ServiceRow> = sqlx::query_as(&format!(
        "{SERVICE_SELECT} \
         WHERE s.workspace_id = (SELECT w.id FROM workspaces w WHERE w.slug = $1) \
         AND s.deleted_at IS NULL \
         AND ($2::uuid[] IS NULL OR s.id = ANY($2)) \
         AND ($3::uuid IS NULL OR EXISTS (SELECT 1 FROM service_project_links spl \
               WHERE spl.service_id = s.id AND spl.project_id = $3 AND spl.deleted_at IS NULL \
               AND ($4::text IS NULL OR spl.link_type = $4))) \
         AND ($5::text IS NULL OR s.name ILIKE '%' || $5 || '%') \
         AND ($6::text IS NULL OR s.status = $6) \
         AND ($7::text IS NULL OR s.criticality = $7) \
         ORDER BY s.sort_order, s.name"
    ))
    .bind(&slug)
    .bind(visible)
    .bind(params.project)
    .bind(params.link)
    .bind(params.q)
    .bind(params.status)
    .bind(params.criticality)
    .fetch_all(&st.pool)
    .await?;
    json_service_list(&st.pool, &rows, "full").await
}

pub async fn detail_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !service_access::full_visible(&st.pool, auth.0, &slug, pk).await? {
        return Ok(missing());
    }
    match service_json_by_id(&st.pool, pk, "full").await? {
        Some(v) => Ok((StatusCode::OK, Json(v))),
        None => Ok(missing()),
    }
}

// ---------------------------------------------------------------------------
// Gates + request bodies
// ---------------------------------------------------------------------------

pub(crate) async fn gate_member(
    pool: &sqlx::PgPool,
    user: Uuid,
    slug: &str,
    project_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(
        matches!(role, Some(20) | Some(15) | Some(5)),
        role.is_some(),
        ws_admin,
    ))
}

pub(crate) async fn gate_writer(
    pool: &sqlx::PgPool,
    user: Uuid,
    slug: &str,
    project_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let role = fetch_project_member_role(pool, user, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user, slug).await?;
    Ok(project_gate_allows(
        matches!(role, Some(20) | Some(15)),
        role.is_some(),
        ws_admin,
    ))
}

/// Deserialize a present-but-null field as `Some(None)` so PATCH can clear it.
pub(crate) fn deserialize_present<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    T::deserialize(de).map(Some)
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateService {
    pub name: Option<String>,
    pub description: Option<String>,
    pub description_html: Option<String>,
    pub status: Option<String>,
    pub criticality: Option<String>,
    #[serde(rename = "type")]
    pub service_type: Option<String>,
    pub owner_id: Option<Uuid>,
    pub repository_url: Option<String>,
    pub documentation_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PatchService {
    pub name: Option<String>,
    pub description: Option<String>,
    pub description_html: Option<String>,
    pub status: Option<String>,
    pub criticality: Option<String>,
    #[serde(rename = "type")]
    pub service_type: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub owner_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub repository_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub documentation_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub position: Option<Option<Value>>,
}

pub(crate) fn bad_request(msg: impl Into<String>) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": msg.into() })),
    )
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let rows: Vec<ServiceRow> = sqlx::query_as(&format!(
        "{SERVICE_SELECT} WHERE s.project_id = $1 AND s.deleted_at IS NULL \
         ORDER BY s.sort_order ASC, s.created_at ASC"
    ))
    .bind(project_id)
    .fetch_all(&st.pool)
    .await?;
    json_service_list(&st.pool, &rows, "full").await
}

pub async fn create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
    Json(body): Json<CreateService>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let name = body
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "Untitled service".to_string());
    let status = body.status.clone().unwrap_or_else(|| "planned".to_string());
    let criticality = body
        .criticality
        .clone()
        .unwrap_or_else(|| "medium".to_string());
    let service_type = body
        .service_type
        .clone()
        .unwrap_or_else(|| "internal".to_string());
    if let Err(e) = validate_enum("status", &status, SERVICE_STATUSES) {
        return Ok(bad_request(e));
    }
    if let Err(e) = validate_enum("criticality", &criticality, SERVICE_CRITICALITIES) {
        return Ok(bad_request(e));
    }
    if let Err(e) = validate_enum("type", &service_type, SERVICE_TYPES) {
        return Ok(bad_request(e));
    }
    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM services WHERE project_id = $1 \
         AND lower(btrim(name)) = lower(btrim($2)) AND deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(&name)
    .fetch_one(&st.pool)
    .await?;
    if dup {
        return Ok(bad_request("A service with this name already exists."));
    }
    let owner_id = body.owner_id;
    if let Some(owner) = owner_id {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
            .bind(owner)
            .fetch_one(&st.pool)
            .await?;
        if !exists {
            return Ok(bad_request(format!(
                "Invalid owner_id \"{owner}\" - object does not exist."
            )));
        }
    }
    let max_order: f64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sort_order), 0) FROM services WHERE project_id = $1 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .fetch_one(&st.pool)
    .await?;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO services (id, workspace_id, project_id, name, description, description_html, \
         status, criticality, \"type\", owner_id, repository_url, documentation_url, position, \
         sort_order, created_at, updated_at, created_by_id, updated_by_id) \
         SELECT $1, p.workspace_id, p.id, $2, $3, $4, $5, $6, $7, $8, $9, $10, NULL, $11, \
         now(), now(), $12, $12 FROM projects p WHERE p.id = $13",
    )
    .bind(id)
    .bind(&name)
    .bind(body.description.clone().unwrap_or_default())
    .bind(body.description_html.clone().unwrap_or_default())
    .bind(&status)
    .bind(&criticality)
    .bind(&service_type)
    .bind(owner_id)
    .bind(body.repository_url.clone())
    .bind(body.documentation_url.clone())
    .bind(next_sort_order(max_order))
    .bind(auth.0)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    let row: ServiceRow = sqlx::query_as(&format!("{SERVICE_SELECT} WHERE s.id = $1"))
        .bind(id)
        .fetch_one(&st.pool)
        .await?;
    let links = links_for_services(&st.pool, &[id]).await?;
    Ok((StatusCode::CREATED, Json(service_json(&row, &links, "full"))))
}

pub async fn detail(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let row: Option<ServiceRow> = sqlx::query_as(&format!(
        "{SERVICE_SELECT} WHERE s.id = $1 AND s.project_id = $2 AND s.deleted_at IS NULL"
    ))
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    match service_json_by_id(&st.pool, pk, "full").await? {
        // Preserve the old not-found semantics when the id exists but is not
        // in this project.
        Some(_) if row.is_none() => Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Service not found"})),
        )),
        Some(v) => Ok((StatusCode::OK, Json(v))),
        None => Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Service not found"})),
        )),
    }
}

pub async fn patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<PatchService>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let current: Option<ServiceRow> = sqlx::query_as(&format!(
        "{SERVICE_SELECT} WHERE s.id = $1 AND s.project_id = $2 AND s.deleted_at IS NULL"
    ))
    .bind(pk)
    .bind(project_id)
    .fetch_optional(&st.pool)
    .await?;
    let Some(current) = current else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Service not found"})),
        ));
    };

    let name = body.name.clone().unwrap_or_else(|| current.name.clone());
    if name.trim().is_empty() {
        return Ok(bad_request("Invalid name"));
    }
    let status = body
        .status
        .clone()
        .unwrap_or_else(|| current.status.clone());
    let criticality = body
        .criticality
        .clone()
        .unwrap_or_else(|| current.criticality.clone());
    let service_type = body
        .service_type
        .clone()
        .unwrap_or_else(|| current.service_type.clone());
    if let Err(e) = validate_enum("status", &status, SERVICE_STATUSES) {
        return Ok(bad_request(e));
    }
    if let Err(e) = validate_enum("criticality", &criticality, SERVICE_CRITICALITIES) {
        return Ok(bad_request(e));
    }
    if let Err(e) = validate_enum("type", &service_type, SERVICE_TYPES) {
        return Ok(bad_request(e));
    }
    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM services WHERE project_id = $1 \
         AND lower(btrim(name)) = lower(btrim($2)) AND id != $3 AND deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(&name)
    .bind(pk)
    .fetch_one(&st.pool)
    .await?;
    if dup {
        return Ok(bad_request("A service with this name already exists."));
    }

    let description = body
        .description
        .clone()
        .unwrap_or_else(|| current.description.clone());
    let description_html = body
        .description_html
        .clone()
        .unwrap_or_else(|| current.description_html.clone());
    let owner_id = match body.owner_id {
        Some(v) => v,
        None => current.owner_id,
    };
    if let Some(owner) = owner_id {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
            .bind(owner)
            .fetch_one(&st.pool)
            .await?;
        if !exists {
            return Ok(bad_request(format!(
                "Invalid owner_id \"{owner}\" - object does not exist."
            )));
        }
    }
    let repository_url = match body.repository_url {
        Some(v) => v,
        None => current.repository_url.clone(),
    };
    let documentation_url = match body.documentation_url {
        Some(v) => v,
        None => current.documentation_url.clone(),
    };
    let position = match body.position {
        Some(v) => v,
        None => current.position.clone(),
    };

    sqlx::query(
        "UPDATE services SET name = $1, description = $2, description_html = $3, status = $4, \
         criticality = $5, \"type\" = $6, owner_id = $7, repository_url = $8, documentation_url = $9, \
         position = $10, updated_at = now(), updated_by_id = $11 \
         WHERE id = $12 AND project_id = $13 AND deleted_at IS NULL",
    )
    .bind(&name)
    .bind(&description)
    .bind(&description_html)
    .bind(&status)
    .bind(&criticality)
    .bind(&service_type)
    .bind(owner_id)
    .bind(&repository_url)
    .bind(&documentation_url)
    .bind(&position)
    .bind(auth.0)
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?;

    let row: Option<ServiceRow> = sqlx::query_as(&format!("{SERVICE_SELECT} WHERE s.id = $1"))
        .bind(pk)
        .fetch_optional(&st.pool)
        .await?;
    let links = links_for_services(&st.pool, &[pk]).await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(service_json(&r, &links, "full")))),
        None => Ok(missing()),
    }
}

pub async fn destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE service_dependencies SET deleted_at = now(), updated_at = now() \
         WHERE project_id = $1 AND (from_service_id = $2 OR to_service_id = $2) AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(pk)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE service_issues SET deleted_at = now(), updated_at = now() \
         WHERE project_id = $1 AND service_id = $2 AND deleted_at IS NULL",
    )
    .bind(project_id)
    .bind(pk)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE services SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    // Idempotent: a missing service still returns 204 (matches the mock).
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DependencyRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub from_service_id: Uuid,
    pub to_service_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

fn dependency_json(row: &DependencyRow) -> Value {
    json!({
        "id": row.id,
        "workspace_id": row.workspace_id,
        "project_id": row.project_id,
        "from_service_id": row.from_service_id,
        "to_service_id": row.to_service_id,
        "created_at": row.created_at,
    })
}

const DEPENDENCY_SELECT: &str = "SELECT id, workspace_id, project_id, from_service_id, \
    to_service_id, created_at FROM service_dependencies";

#[derive(Debug, Clone, Deserialize)]
pub struct CreateDependency {
    pub from_service_id: Uuid,
    pub to_service_id: Uuid,
}

async fn service_exists(
    pool: &sqlx::PgPool,
    project_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM services WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)",
    )
    .bind(id)
    .bind(project_id)
    .fetch_one(pool)
    .await
}

pub async fn dependencies_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let rows: Vec<DependencyRow> = sqlx::query_as(&format!(
        "{DEPENDENCY_SELECT} WHERE project_id = $1 AND deleted_at IS NULL ORDER BY created_at ASC"
    ))
    .bind(project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(dependency_json).collect())),
    ))
}

pub async fn dependencies_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
    Json(body): Json<CreateDependency>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    if !service_exists(&st.pool, project_id, body.from_service_id).await? {
        return Ok(bad_request("Source service not found."));
    }
    if !service_exists(&st.pool, project_id, body.to_service_id).await? {
        return Ok(bad_request("Target service not found."));
    }
    if body.from_service_id == body.to_service_id {
        return Ok(bad_request("A service cannot depend on itself."));
    }
    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM service_dependencies WHERE project_id = $1 \
         AND from_service_id = $2 AND to_service_id = $3 AND deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(body.from_service_id)
    .bind(body.to_service_id)
    .fetch_one(&st.pool)
    .await?;
    if dup {
        return Ok(bad_request("This dependency already exists."));
    }
    // Adding from -> to creates a cycle when `from` is reachable from `to`.
    let creates_cycle: bool = sqlx::query_scalar(
        "WITH RECURSIVE reach(node) AS ( \
           SELECT to_service_id FROM service_dependencies \
             WHERE project_id = $1 AND from_service_id = $2 AND deleted_at IS NULL \
           UNION \
           SELECT d.to_service_id FROM service_dependencies d \
             JOIN reach r ON d.from_service_id = r.node \
             WHERE d.project_id = $1 AND d.deleted_at IS NULL \
         ) SELECT EXISTS(SELECT 1 FROM reach WHERE node = $3)",
    )
    .bind(project_id)
    .bind(body.to_service_id)
    .bind(body.from_service_id)
    .fetch_one(&st.pool)
    .await?;
    if creates_cycle {
        return Ok(bad_request("This dependency would create a cycle."));
    }
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO service_dependencies (id, workspace_id, project_id, from_service_id, \
         to_service_id, created_at, updated_at, created_by_id, updated_by_id) \
         SELECT $1, p.workspace_id, p.id, $2, $3, now(), now(), $4, $4 FROM projects p WHERE p.id = $5",
    )
    .bind(id)
    .bind(body.from_service_id)
    .bind(body.to_service_id)
    .bind(auth.0)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    let row: DependencyRow = sqlx::query_as(&format!("{DEPENDENCY_SELECT} WHERE id = $1"))
        .bind(id)
        .fetch_one(&st.pool)
        .await?;
    Ok((StatusCode::CREATED, Json(dependency_json(&row))))
}

pub async fn dependency_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    sqlx::query(
        "UPDATE service_dependencies SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

// ---------------------------------------------------------------------------
// Work-item links
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ServiceIssueRow {
    pub id: Uuid,
    pub service_id: Uuid,
    pub issue_id: Uuid,
    pub project_id: Uuid,
    pub workspace_id: Uuid,
    pub issue_identifier: String,
    pub issue_name: String,
}

fn service_issue_json(row: &ServiceIssueRow) -> Value {
    json!({
        "id": row.id,
        "service_id": row.service_id,
        "issue_id": row.issue_id,
        "project_id": row.project_id,
        "workspace_id": row.workspace_id,
        "issue_identifier": row.issue_identifier,
        "issue_name": row.issue_name,
    })
}

const SERVICE_ISSUE_SELECT: &str = "SELECT si.id, si.service_id, si.issue_id, si.project_id, \
    si.workspace_id, (p.identifier || '-' || i.sequence_id::text) AS issue_identifier, \
    i.name AS issue_name FROM service_issues si \
    JOIN issues i ON i.id = si.issue_id JOIN projects p ON p.id = si.project_id";

#[derive(Debug, Clone, Deserialize)]
pub struct CreateServiceIssue {
    pub service_id: Uuid,
    pub issue_id: Uuid,
}

pub async fn issues_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let rows: Vec<ServiceIssueRow> = sqlx::query_as(&format!(
        "{SERVICE_ISSUE_SELECT} WHERE si.project_id = $1 AND si.deleted_at IS NULL \
         ORDER BY si.created_at ASC"
    ))
    .bind(project_id)
    .fetch_all(&st.pool)
    .await?;
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(service_issue_json).collect())),
    ))
}

pub async fn issues_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, Uuid)>,
    Json(body): Json<CreateServiceIssue>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    if !service_exists(&st.pool, project_id, body.service_id).await? {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Service not found."})),
        ));
    }
    let issue_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL)",
    )
    .bind(body.issue_id)
    .bind(project_id)
    .fetch_one(&st.pool)
    .await?;
    if !issue_exists {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Issue not found."})),
        ));
    }
    let existing: Option<ServiceIssueRow> = sqlx::query_as(&format!(
        "{SERVICE_ISSUE_SELECT} WHERE si.project_id = $1 AND si.service_id = $2 \
         AND si.issue_id = $3 AND si.deleted_at IS NULL"
    ))
    .bind(project_id)
    .bind(body.service_id)
    .bind(body.issue_id)
    .fetch_optional(&st.pool)
    .await?;
    if let Some(row) = existing {
        return Ok((StatusCode::CREATED, Json(service_issue_json(&row))));
    }
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO service_issues (id, workspace_id, project_id, service_id, issue_id, \
         created_at, updated_at, created_by_id, updated_by_id) \
         SELECT $1, p.workspace_id, p.id, $2, $3, now(), now(), $4, $4 FROM projects p WHERE p.id = $5",
    )
    .bind(id)
    .bind(body.service_id)
    .bind(body.issue_id)
    .bind(auth.0)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    let row: ServiceIssueRow = sqlx::query_as(&format!("{SERVICE_ISSUE_SELECT} WHERE si.id = $1"))
        .bind(id)
        .fetch_one(&st.pool)
        .await?;
    Ok((StatusCode::CREATED, Json(service_issue_json(&row))))
}

pub async fn issue_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_writer(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    sqlx::query(
        "UPDATE service_issues SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND project_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(project_id)
    .execute(&st.pool)
    .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_name_trims_and_lowercases() {
        assert_eq!(normalize_name("  Web API "), "web api");
    }

    #[test]
    fn validate_enum_accepts_known_and_rejects_unknown() {
        assert!(validate_enum("status", "active", SERVICE_STATUSES).is_ok());
        assert_eq!(
            validate_enum("status", "bogus", SERVICE_STATUSES).unwrap_err(),
            "Invalid status"
        );
        assert_eq!(
            validate_enum("criticality", "nope", SERVICE_CRITICALITIES).unwrap_err(),
            "Invalid criticality"
        );
        assert_eq!(
            validate_enum("type", "nope", SERVICE_TYPES).unwrap_err(),
            "Invalid type"
        );
    }

    #[test]
    fn cycle_detection_self_and_transitive() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let c = Uuid::from_u128(3);
        // existing: a -> b, b -> c (a depends on b, b depends on c)
        let edges = vec![(a, b), (b, c)];
        assert!(would_create_cycle(&edges, a, a));
        assert!(would_create_cycle(&edges, c, a));
        assert!(!would_create_cycle(&edges, a, c));
    }

    #[test]
    fn next_sort_order_appends() {
        assert_eq!(next_sort_order(0.0), 65535.0);
        assert_eq!(next_sort_order(65535.0), 131070.0);
    }
}
