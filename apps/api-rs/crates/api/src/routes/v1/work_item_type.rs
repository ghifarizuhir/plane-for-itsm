//! v1 work-item-type data layer (`issue_types` + `project_issue_types`).
//! Handlers arrive in later tasks; this module holds only the row shape,
//! JSON shaper, create/update bodies, and auth helpers.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::FromRow;

use crate::routes::member::deny_detail;
use crate::routes::project::{deny, missing, project_role, ws_role};
use crate::routes::v1::common::PageParams;
use crate::{middleware::auth::AuthUser, state::AppState};

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

/// Column list for `issue_types t` (workspace-owned types; project links come
/// from `project_issue_types`). `level` is `double precision` in the DB, cast
/// to int for the SDK shape.
pub const TYPE_COLS: &str = "t.id, t.name, t.description, t.logo_props, t.is_epic, t.is_default, t.is_active, t.level::int AS level, t.external_id, t.external_source, t.created_by_id AS created_by, t.updated_by_id AS updated_by, t.workspace_id AS workspace, t.created_at, t.updated_at, t.deleted_at, COALESCE((SELECT array_agg(pit.project_id) FROM project_issue_types pit WHERE pit.issue_type_id = t.id AND pit.deleted_at IS NULL), ARRAY[]::uuid[]) AS project_ids";

#[derive(Debug, Clone, FromRow)]
pub struct V1WorkItemTypeRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub description: Option<String>,
    pub logo_props: Option<Value>,
    pub is_epic: bool,
    pub is_default: bool,
    pub is_active: bool,
    pub level: Option<i32>,
    pub external_id: Option<String>,
    pub external_source: Option<String>,
    pub created_by: Option<uuid::Uuid>,
    pub updated_by: Option<uuid::Uuid>,
    pub workspace: uuid::Uuid,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub project_ids: Vec<uuid::Uuid>,
}

pub fn v1_work_item_type_json(row: &V1WorkItemTypeRow) -> Value {
    json!({
        "id": row.id,
        "name": row.name,
        "description": row.description,
        "logo_props": row.logo_props.clone().unwrap_or(Value::Null),
        "is_epic": row.is_epic,
        "is_default": row.is_default,
        "is_active": row.is_active,
        "level": row.level,
        "external_id": row.external_id,
        "external_source": row.external_source,
        "created_by": row.created_by,
        "updated_by": row.updated_by,
        "workspace": row.workspace,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "deleted_at": row.deleted_at,
        "project_ids": row.project_ids,
    })
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct V1CreateWorkItemType {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub logo_props: Option<Value>,
    #[serde(default)]
    pub is_epic: Option<bool>,
    #[serde(default)]
    pub is_active: Option<bool>,
    #[serde(default)]
    pub level: Option<i32>,
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub external_source: Option<String>,
    #[serde(default)]
    pub project_ids: Vec<uuid::Uuid>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct V1UpdateWorkItemType {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub logo_props: Option<Value>,
    #[serde(default)]
    pub is_epic: Option<bool>,
    #[serde(default)]
    pub is_active: Option<bool>,
    #[serde(default)]
    pub level: Option<i32>,
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub external_source: Option<String>,
    #[serde(default)]
    pub project_ids: Option<Vec<uuid::Uuid>>,
}

pub(crate) async fn ws_id(
    pool: &sqlx::PgPool,
    slug: &str,
) -> Result<Option<uuid::Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM workspaces WHERE slug = $1 AND deleted_at IS NULL")
        .bind(slug)
        .fetch_optional(pool)
        .await
}

/// Write gate: workspace admin+ (`ws_role >= 20`); otherwise the caller must
/// be a project admin (`project_role == 20`) when a project is in scope.
pub(crate) async fn can_write(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    slug: &str,
    project: Option<uuid::Uuid>,
) -> Result<bool, sqlx::Error> {
    if matches!(ws_role(pool, user_id, slug).await?, Some(r) if r >= 20) {
        return Ok(true);
    }
    if let Some(pid) = project {
        if matches!(project_role(pool, user_id, pid).await?, Some(20)) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub async fn list_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(_q): Query<PageParams>,
) -> R {
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny_detail());
    }
    let sql = format!(
        "SELECT {TYPE_COLS} FROM issue_types t \
         WHERE t.workspace_id = (SELECT id FROM workspaces WHERE slug = $1) \
         AND t.deleted_at IS NULL ORDER BY t.created_at ASC"
    );
    let rows: Vec<V1WorkItemTypeRow> = sqlx::query_as(&sql).bind(&slug).fetch_all(&st.pool).await?;
    let out: Vec<Value> = rows.iter().map(v1_work_item_type_json).collect();
    // Bare array: the SDK iterates this response.
    Ok((StatusCode::OK, Json(Value::Array(out))))
}

pub async fn create_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<V1CreateWorkItemType>,
) -> R {
    if !can_write(&st.pool, auth.0, &slug, None).await? {
        return Ok(deny());
    }
    create_type(&st, auth.0, &slug, None, body).await
}

pub async fn retrieve_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
) -> R {
    if ws_role(&st.pool, auth.0, &slug).await?.is_none() {
        return Ok(deny_detail());
    }
    let sql = format!(
        "SELECT {TYPE_COLS} FROM issue_types t \
         WHERE t.id = $1 AND t.workspace_id = (SELECT id FROM workspaces WHERE slug = $2) \
         AND t.deleted_at IS NULL"
    );
    let row: Option<V1WorkItemTypeRow> = sqlx::query_as(&sql)
        .bind(pk)
        .bind(&slug)
        .fetch_optional(&st.pool)
        .await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(v1_work_item_type_json(&r)))),
        None => Ok(missing()),
    }
}

pub async fn update_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
    Json(body): Json<V1UpdateWorkItemType>,
) -> R {
    if !can_write(&st.pool, auth.0, &slug, None).await? {
        return Ok(deny());
    }
    update_type(&st, auth.0, &slug, None, pk, body).await
}

pub async fn delete_workspace(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, pk)): Path<(String, uuid::Uuid)>,
) -> R {
    if !can_write(&st.pool, auth.0, &slug, None).await? {
        return Ok(deny());
    }
    let affected = sqlx::query(
        "UPDATE issue_types SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND workspace_id = (SELECT id FROM workspaces WHERE slug = $2) \
         AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(&slug)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if affected == 0 {
        return Ok(missing());
    }
    sqlx::query("UPDATE project_issue_types SET deleted_at = now(), updated_at = now() WHERE issue_type_id = $1 AND deleted_at IS NULL")
        .bind(pk)
        .execute(&st.pool)
        .await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

/// Shared insert used by both scopes. `scope_project` is `Some` for a
/// project-scope create (always linked) and `None` for workspace scope.
async fn create_type(
    st: &AppState,
    user: uuid::Uuid,
    slug: &str,
    scope_project: Option<uuid::Uuid>,
    body: V1CreateWorkItemType,
) -> R {
    let name = body.name.as_deref().map(str::trim).unwrap_or("");
    if name.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "name is required"})),
        ));
    }
    if name.chars().count() > 255 {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "name max length 255"})),
        ));
    }
    let Some(ws) = ws_id(&st.pool, slug).await? else {
        return Ok(missing());
    };
    let row: V1WorkItemTypeRow = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, \
         is_active, level, external_id, external_source, workspace_id, created_by_id, updated_by_id, \
         created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, false, $5, $6, $7, $8, $9, $10, $10, now(), now()) \
         RETURNING id, name, description, logo_props, is_epic, is_default, is_active, \
         level::int AS level, external_id, external_source, created_by_id AS created_by, \
         updated_by_id AS updated_by, workspace_id AS workspace, created_at, updated_at, deleted_at, \
         ARRAY[]::uuid[] AS project_ids",
    )
    .bind(name)
    .bind(body.description.clone().unwrap_or_default())
    .bind(body.logo_props.clone().unwrap_or_else(|| json!({})))
    .bind(body.is_epic.unwrap_or(false))
    .bind(body.is_active.unwrap_or(true))
    .bind(body.level.unwrap_or(0) as f64)
    .bind(body.external_id.clone())
    .bind(body.external_source.clone())
    .bind(ws)
    .bind(user)
    .fetch_one(&st.pool)
    .await?;

    let mut link_ids = body.project_ids.clone();
    if let Some(pid) = scope_project {
        link_ids.push(pid);
    }
    link_projects(st, &ws, row.id, user, &link_ids).await?;

    let row = reload(st, &ws, row.id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok((StatusCode::CREATED, Json(v1_work_item_type_json(&row))))
}

/// Insert `project_issue_types` links for types/projects both in `ws`,
/// skipping deleted projects and existing live links.
async fn link_projects(
    st: &AppState,
    ws: &uuid::Uuid,
    type_id: uuid::Uuid,
    user: uuid::Uuid,
    project_ids: &[uuid::Uuid],
) -> Result<(), common::errors::AppError> {
    for pid in project_ids {
        sqlx::query(
            "INSERT INTO project_issue_types (id, issue_type_id, project_id, workspace_id, \
             level, is_default, created_by_id, updated_by_id, created_at, updated_at) \
             SELECT gen_random_uuid(), $1, p.id, $2, 0, false, $3, $3, now(), now() \
             FROM projects p WHERE p.id = $4 AND p.workspace_id = $2 AND p.deleted_at IS NULL \
             AND NOT EXISTS(SELECT 1 FROM project_issue_types pit \
               WHERE pit.project_id = p.id AND pit.issue_type_id = $1 AND pit.deleted_at IS NULL)",
        )
        .bind(type_id)
        .bind(ws)
        .bind(user)
        .bind(pid)
        .execute(&st.pool)
        .await?;
    }
    Ok(())
}

async fn reload(
    st: &AppState,
    ws: &uuid::Uuid,
    id: uuid::Uuid,
) -> Result<Option<V1WorkItemTypeRow>, common::errors::AppError> {
    let sql = format!(
        "SELECT {TYPE_COLS} FROM issue_types t WHERE t.id = $1 AND t.workspace_id = $2 AND t.deleted_at IS NULL"
    );
    Ok(sqlx::query_as(&sql)
        .bind(id)
        .bind(ws)
        .fetch_optional(&st.pool)
        .await?)
}

/// Shared update used by both scopes. `scope_project` restricts the row to a
/// project (via a live `project_issue_types` link) when `Some`.
async fn update_type(
    st: &AppState,
    user: uuid::Uuid,
    slug: &str,
    scope_project: Option<uuid::Uuid>,
    pk: uuid::Uuid,
    body: V1UpdateWorkItemType,
) -> R {
    let Some(ws) = ws_id(&st.pool, slug).await? else {
        return Ok(missing());
    };
    let in_scope: bool = match scope_project {
        Some(pid) => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM project_issue_types pit JOIN issue_types t ON t.id = pit.issue_type_id \
             WHERE pit.issue_type_id = $1 AND pit.project_id = $2 AND pit.deleted_at IS NULL \
             AND t.workspace_id = $3 AND t.deleted_at IS NULL)",
        )
        .bind(pk)
        .bind(pid)
        .bind(ws)
        .fetch_one(&st.pool)
        .await?,
        None => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM issue_types WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL)",
        )
        .bind(pk)
        .bind(ws)
        .fetch_one(&st.pool)
        .await?,
    };
    if !in_scope {
        return Ok(missing());
    }

    if let Some(n) = body.name.as_deref().map(str::trim) {
        if n.is_empty() {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "name is required"})),
            ));
        }
    }
    // Column-by-column COALESCE so omitted fields are untouched.
    let updated = sqlx::query(
        "UPDATE issue_types SET \
         name = COALESCE($2, name), \
         description = COALESCE($3, description), \
         logo_props = COALESCE($4, logo_props), \
         is_epic = COALESCE($5, is_epic), \
         is_active = COALESCE($6, is_active), \
         level = COALESCE($7, level), \
         external_id = COALESCE($8, external_id), \
         external_source = COALESCE($9, external_source), \
         updated_by_id = $10, updated_at = now() \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(body.name.as_deref().map(str::trim))
    .bind(body.description.clone())
    .bind(body.logo_props.clone())
    .bind(body.is_epic)
    .bind(body.is_active)
    .bind(body.level.map(|l| l as f64))
    .bind(body.external_id.clone())
    .bind(body.external_source.clone())
    .bind(user)
    .execute(&st.pool)
    .await?;
    if updated.rows_affected() == 0 {
        return Ok(missing());
    }
    if let Some(ids) = body.project_ids.as_ref() {
        link_projects(st, &ws, pk, user, ids).await?;
    }
    match reload(st, &ws, pk).await? {
        Some(r) => Ok((StatusCode::OK, Json(v1_work_item_type_json(&r)))),
        None => Ok(missing()),
    }
}
