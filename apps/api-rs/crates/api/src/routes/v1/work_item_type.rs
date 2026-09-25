//! v1 work-item-type handlers (`issue_types` + `project_issue_types`).
//! Row shape, JSON shaper, create/update bodies, auth helpers, plus
//! workspace and project CRUD (list/create/retrieve/update/delete) and
//! project import (link existing types into a project).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::FromRow;

use crate::routes::issue_common::bad;
use crate::routes::member::deny_detail;
use crate::routes::project::{deny, fetch_project_full, missing, project_role, ws_role};
use crate::routes::v1::common::PageParams;
use crate::{middleware::auth::AuthUser, state::AppState};

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

/// Column list for `issue_types t` (workspace-owned types; project links come
/// from `project_issue_types`). `level` is `double precision` in the DB, cast
/// to int for the SDK shape.
pub const TYPE_COLS: &str = "t.id, t.name, t.description, t.logo_props, t.is_epic, t.is_default, t.is_active, t.level::int AS level, t.workflow_id AS workflow, t.external_id, t.external_source, t.created_by_id AS created_by, t.updated_by_id AS updated_by, t.workspace_id AS workspace, t.created_at, t.updated_at, t.deleted_at, COALESCE((SELECT array_agg(pit.project_id) FROM project_issue_types pit WHERE pit.issue_type_id = t.id AND pit.deleted_at IS NULL), ARRAY[]::uuid[]) AS project_ids";

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
    pub workflow: Option<uuid::Uuid>,
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
        "workflow": row.workflow,
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
    pub workflow: Option<uuid::Uuid>,
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub external_source: Option<String>,
    #[serde(default)]
    pub project_ids: Vec<uuid::Uuid>,
}

/// Membedakan field `workflow` yang hilang (`None`) dari `null` eksplisit
/// (`Some(None)`); nilai biasa menjadi `Some(Some(id))`.
fn deserialize_optional_nullable<'de, D>(
    deserializer: D,
) -> Result<Option<Option<uuid::Uuid>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<uuid::Uuid>::deserialize(deserializer)?))
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
    #[serde(default, deserialize_with = "deserialize_optional_nullable")]
    pub workflow: Option<Option<uuid::Uuid>>,
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
    let (in_use,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE type_id = $1 AND deleted_at IS NULL)",
    )
    .bind(pk)
    .fetch_one(&st.pool)
    .await?;
    if in_use {
        return Ok(bad("Type is in use by work items"));
    }
    let (linked,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM project_issue_types WHERE issue_type_id = $1 AND deleted_at IS NULL)",
    )
    .bind(pk)
    .fetch_one(&st.pool)
    .await?;
    if linked {
        return Ok(bad("Type is enabled in projects"));
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
    let mut link_ids = body.project_ids.clone();
    if let Some(pid) = scope_project {
        link_ids.push(pid);
    }
    if body.is_epic.unwrap_or(false) && body.workflow.is_some() {
        return Ok(bad("Epic types cannot have a workflow"));
    }
    // Guard sebelum INSERT agar create yang konflik tidak meninggalkan type
    // orphan tanpa link.
    if let Some(workflow_id) = body.workflow {
        if !workflow_in_workspace(&st.pool, ws, workflow_id).await? {
            return Ok(bad("Workflow does not exist in this workspace"));
        }
        if workflow_link_conflict(&st.pool, &link_ids, workflow_id, None).await? {
            return Ok(bad(
                "Workflow is already enabled for another work item type in this project",
            ));
        }
    }
    let row: V1WorkItemTypeRow = sqlx::query_as(
        "INSERT INTO issue_types (id, name, description, logo_props, is_epic, is_default, \
         is_active, level, workflow_id, external_id, external_source, workspace_id, created_by_id, \
         updated_by_id, created_at, updated_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, false, $5, $6, $7, $8, $9, $10, $11, $11, now(), now()) \
         RETURNING id, name, description, logo_props, is_epic, is_default, is_active, \
         level::int AS level, workflow_id AS workflow, external_id, external_source, \
         created_by_id AS created_by, updated_by_id AS updated_by, workspace_id AS workspace, \
         created_at, updated_at, deleted_at, ARRAY[]::uuid[] AS project_ids",
    )
    .bind(name)
    .bind(body.description.clone().unwrap_or_default())
    .bind(body.logo_props.clone().unwrap_or_else(|| json!({})))
    .bind(body.is_epic.unwrap_or(false))
    .bind(body.is_active.unwrap_or(true))
    .bind(body.level.unwrap_or(0) as f64)
    .bind(body.workflow)
    .bind(body.external_id.clone())
    .bind(body.external_source.clone())
    .bind(ws)
    .bind(user)
    .fetch_one(&st.pool)
    .await?;

    let linked = link_projects(st, &ws, row.id, user, &link_ids).await?;
    if row.workflow.is_some() {
        for pid in &linked {
            crate::routes::workflow::materialize_type_states(&st.pool, *pid, row.id).await?;
        }
    }

    let row = reload(st, &ws, row.id)
        .await?
        .ok_or_else(|| common::errors::AppError::internal())?;
    Ok((StatusCode::CREATED, Json(v1_work_item_type_json(&row))))
}

/// Guard invariant "satu workflow per type per project" untuk jalur link
/// `project_ids`: `true` bila salah satu `project_ids` sudah punya type hidup
/// lain yang memakai `workflow_id` (opsional mengecualikan type itu sendiri).
async fn workflow_link_conflict(
    pool: &sqlx::PgPool,
    project_ids: &[uuid::Uuid],
    workflow_id: uuid::Uuid,
    exclude_type_id: Option<uuid::Uuid>,
) -> Result<bool, common::errors::AppError> {
    for project_id in project_ids {
        let (conflict,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM project_issue_types pit \
             JOIN issue_types t ON t.id = pit.issue_type_id \
             JOIN projects p ON p.id = pit.project_id \
             WHERE pit.project_id = $1 AND pit.deleted_at IS NULL AND t.deleted_at IS NULL \
               AND p.deleted_at IS NULL \
               AND t.workflow_id = $2 AND ($3::uuid IS NULL OR t.id <> $3))",
        )
        .bind(project_id)
        .bind(workflow_id)
        .bind(exclude_type_id)
        .fetch_one(pool)
        .await?;
        if conflict {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `true` bila `workflow_id` adalah workflow hidup di `workspace_id`.
async fn workflow_in_workspace(
    pool: &sqlx::PgPool,
    workspace_id: uuid::Uuid,
    workflow_id: uuid::Uuid,
) -> Result<bool, common::errors::AppError> {
    let (ok,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM workflows WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL)",
    )
    .bind(workflow_id)
    .bind(workspace_id)
    .fetch_one(pool)
    .await?;
    Ok(ok)
}

/// Insert `project_issue_types` links for types/projects both in `ws`,
/// skipping deleted projects and existing live links. Returns the requested
/// projects that hold a live link afterwards (new or pre-existing).
async fn link_projects(
    st: &AppState,
    ws: &uuid::Uuid,
    type_id: uuid::Uuid,
    user: uuid::Uuid,
    project_ids: &[uuid::Uuid],
) -> Result<Vec<uuid::Uuid>, common::errors::AppError> {
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
    let linked: Vec<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT pit.project_id FROM project_issue_types pit \
         JOIN projects p ON p.id = pit.project_id \
         WHERE pit.issue_type_id = $1 AND pit.project_id = ANY($2) \
           AND pit.deleted_at IS NULL AND p.workspace_id = $3 AND p.deleted_at IS NULL",
    )
    .bind(type_id)
    .bind(project_ids)
    .bind(ws)
    .fetch_all(&st.pool)
    .await?;
    Ok(linked.into_iter().map(|(pid,)| pid).collect())
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
    // Workflow: omitted (`None`) = unchanged, `Some(None)` = clear, dan
    // `Some(Some(wf))` = set/switch. Query ini sekaligus membawa `is_epic`
    // efektif.
    let current: Option<(Option<uuid::Uuid>, bool)> = sqlx::query_as(
        "SELECT workflow_id, is_epic FROM issue_types WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(pk)
    .fetch_optional(&st.pool)
    .await?;
    let Some((current_workflow, current_is_epic)) = current else {
        return Ok(missing());
    };
    let set_workflow = match body.workflow {
        Some(Some(wf)) => Some(wf),
        _ => None,
    };
    let clear_workflow = matches!(body.workflow, Some(None));
    let effective_workflow: Option<uuid::Uuid> = match body.workflow {
        Some(Some(wf)) => Some(wf),
        Some(None) => None,
        None => current_workflow,
    };
    if body.is_epic.unwrap_or(current_is_epic) && effective_workflow.is_some() {
        return Ok(bad("Epic types cannot have a workflow"));
    }
    if let Some(workflow_id) = set_workflow {
        if !workflow_in_workspace(&st.pool, ws, workflow_id).await? {
            return Ok(bad("Workflow does not exist in this workspace"));
        }
    }
    if clear_workflow {
        let (enabled,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM project_issue_types WHERE issue_type_id = $1 AND deleted_at IS NULL)",
        )
        .bind(pk)
        .fetch_one(&st.pool)
        .await?;
        if enabled {
            return Ok(bad(
                "Cannot unassign a workflow while the type is enabled in projects",
            ));
        }
    }
    // Switch workflow pada type yang sudah enabled: guard ke semua project
    // hidup yang mengaktifkan type ini, lalu materialize setelah UPDATE.
    let switch_to = match set_workflow {
        Some(wf) if current_workflow != Some(wf) => Some(wf),
        _ => None,
    };
    let enabled_project_ids: Vec<uuid::Uuid> = if switch_to.is_some() {
        sqlx::query_scalar(
            "SELECT pit.project_id FROM project_issue_types pit \
             WHERE pit.issue_type_id = $1 AND pit.deleted_at IS NULL",
        )
        .bind(pk)
        .fetch_all(&st.pool)
        .await?
    } else {
        Vec::new()
    };
    if let Some(wf) = switch_to {
        if workflow_link_conflict(&st.pool, &enabled_project_ids, wf, Some(pk)).await? {
            return Ok(bad(
                "Workflow is already enabled for another work item type in this project",
            ));
        }
        let (has_live_issues,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM issues i JOIN states s ON s.id = i.state_id \
             WHERE i.type_id = $1 AND i.deleted_at IS NULL AND s.project_id = ANY($2))",
        )
        .bind(pk)
        .bind(&enabled_project_ids)
        .fetch_one(&st.pool)
        .await?;
        if has_live_issues {
            return Ok(bad(
                "Cannot change the workflow while the type has live work items",
            ));
        }
    }
    if let (Some(ids), Some(workflow_id)) = (body.project_ids.as_ref(), effective_workflow) {
        if workflow_link_conflict(&st.pool, ids, workflow_id, Some(pk)).await? {
            return Ok(bad(
                "Workflow is already enabled for another work item type in this project",
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
         workflow_id = CASE WHEN $8 THEN NULL ELSE COALESCE($9, workflow_id) END, \
         external_id = COALESCE($10, external_id), \
         external_source = COALESCE($11, external_source), \
         updated_by_id = $12, updated_at = now() \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(body.name.as_deref().map(str::trim))
    .bind(body.description.clone())
    .bind(body.logo_props.clone())
    .bind(body.is_epic)
    .bind(body.is_active)
    .bind(body.level.map(|l| l as f64))
    .bind(clear_workflow)
    .bind(set_workflow)
    .bind(body.external_id.clone())
    .bind(body.external_source.clone())
    .bind(user)
    .execute(&st.pool)
    .await?;
    if updated.rows_affected() == 0 {
        return Ok(missing());
    }
    // Materialize union (dedup) dari project yang di-link request dan project
    // enabled saat workflow pindah; supersede cleanup menangani mirror lama.
    let mut materialize_ids: Vec<uuid::Uuid> = Vec::new();
    if let Some(ids) = body.project_ids.as_ref() {
        let linked = link_projects(st, &ws, pk, user, ids).await?;
        if effective_workflow.is_some() {
            materialize_ids.extend(linked);
        }
    }
    if switch_to.is_some() {
        materialize_ids.extend(enabled_project_ids.iter().copied());
    }
    materialize_ids.sort_unstable();
    materialize_ids.dedup();
    for pid in &materialize_ids {
        crate::routes::workflow::materialize_type_states(&st.pool, *pid, pk).await?;
    }
    match reload(st, &ws, pk).await? {
        Some(r) => Ok((StatusCode::OK, Json(v1_work_item_type_json(&r)))),
        None => Ok(missing()),
    }
}

/// Project-scope readability gate, mirroring `v1::project::get_features`:
/// workspace membership required, then project existence (archived counts as
/// missing), then member-or-public visibility.
async fn project_readable(
    st: &AppState,
    user: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<Option<(StatusCode, Json<Value>)>, common::errors::AppError> {
    if ws_role(&st.pool, user, slug).await?.is_none() {
        return Ok(Some(deny_detail()));
    }
    let Some(row) = fetch_project_full(&st.pool, slug, project_id, user).await? else {
        return Ok(Some((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Project does not exist"})),
        )));
    };
    if row.archived_at.is_some() {
        return Ok(Some((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Project does not exist"})),
        )));
    }
    if !row.member_ids.contains(&user) {
        if row.network == 0 {
            return Ok(Some((
                StatusCode::FORBIDDEN,
                Json(json!({"error": "You do not have permission"})),
            )));
        }
        return Ok(Some((
            StatusCode::CONFLICT,
            Json(json!({"error": "You are not a member of this project"})),
        )));
    }
    Ok(None)
}

pub async fn list_project(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
    Query(_q): Query<PageParams>,
) -> R {
    if let Some(gate) = project_readable(&st, auth.0, &slug, project_id).await? {
        return Ok(gate);
    }
    let sql = format!(
        "SELECT {TYPE_COLS} FROM issue_types t \
         JOIN project_issue_types pit ON pit.issue_type_id = t.id AND pit.deleted_at IS NULL \
         WHERE pit.project_id = $1 AND t.deleted_at IS NULL ORDER BY t.created_at ASC"
    );
    let rows: Vec<V1WorkItemTypeRow> = sqlx::query_as(&sql)
        .bind(project_id)
        .fetch_all(&st.pool)
        .await?;
    let out: Vec<Value> = rows.iter().map(v1_work_item_type_json).collect();
    // Bare array: the SDK iterates this response.
    Ok((StatusCode::OK, Json(Value::Array(out))))
}

pub async fn create_project(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
    Json(body): Json<V1CreateWorkItemType>,
) -> R {
    if !can_write(&st.pool, auth.0, &slug, Some(project_id)).await? {
        return Ok(deny());
    }
    if let Some(gate) = project_readable(&st, auth.0, &slug, project_id).await? {
        return Ok(gate);
    }
    create_type(&st, auth.0, &slug, Some(project_id), body).await
}

/// Project-scope link check: a live `project_issue_types` link to a live type,
/// both in the workspace behind `slug`.
async fn project_scope_ok(
    st: &AppState,
    slug: &str,
    project_id: uuid::Uuid,
    pk: uuid::Uuid,
) -> Result<bool, common::errors::AppError> {
    let Some(ws) = ws_id(&st.pool, slug).await? else {
        return Ok(false);
    };
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM project_issue_types pit JOIN issue_types t ON t.id = pit.issue_type_id \
         WHERE pit.issue_type_id = $1 AND pit.project_id = $2 AND pit.deleted_at IS NULL \
         AND t.workspace_id = $3 AND t.deleted_at IS NULL)",
    )
    .bind(pk)
    .bind(project_id)
    .bind(ws)
    .fetch_one(&st.pool)
    .await?)
}

pub async fn retrieve_project(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> R {
    if let Some(gate) = project_readable(&st, auth.0, &slug, project_id).await? {
        return Ok(gate);
    }
    if !project_scope_ok(&st, &slug, project_id, pk).await? {
        return Ok(missing());
    }
    let Some(ws) = ws_id(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    match reload(&st, &ws, pk).await? {
        Some(r) => Ok((StatusCode::OK, Json(v1_work_item_type_json(&r)))),
        None => Ok(missing()),
    }
}

pub async fn update_project(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<V1UpdateWorkItemType>,
) -> R {
    if !can_write(&st.pool, auth.0, &slug, Some(project_id)).await? {
        return Ok(deny());
    }
    if let Some(gate) = project_readable(&st, auth.0, &slug, project_id).await? {
        return Ok(gate);
    }
    update_type(&st, auth.0, &slug, Some(project_id), pk, body).await
}

pub async fn delete_project(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid)>,
) -> R {
    if !can_write(&st.pool, auth.0, &slug, Some(project_id)).await? {
        return Ok(deny());
    }
    if let Some(gate) = project_readable(&st, auth.0, &slug, project_id).await? {
        return Ok(gate);
    }
    let (in_use,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE project_id = $1 AND type_id = $2 AND deleted_at IS NULL)",
    )
    .bind(project_id)
    .bind(pk)
    .fetch_one(&st.pool)
    .await?;
    if in_use {
        return Ok(bad("Type is in use by work items"));
    }
    // Project scope detaches only: soft-delete link + mirror, never the type.
    // Idempotent: project/type tanpa link hidup tetap 204.
    crate::routes::workflow::detach_type_from_project(&st.pool, project_id, pk).await?;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}

pub async fn import_to_project(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id)): Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> R {
    if !can_write(&st.pool, auth.0, &slug, Some(project_id)).await? {
        return Ok(deny());
    }
    if let Some(resp) = project_readable(&st, auth.0, &slug, project_id).await? {
        return Ok(resp);
    }
    let Some(ws) = ws_id(&st.pool, &slug).await? else {
        return Ok(missing());
    };
    let ids: Vec<uuid::Uuid> = body
        .get("work_item_types")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .filter_map(|s| s.parse::<uuid::Uuid>().ok())
                .collect()
        })
        .unwrap_or_default();
    // Guard sebelum link: satu workflow hanya boleh dipakai satu type hidup
    // per project, baik di dalam batch request maupun terhadap link existing.
    let (batch_conflict,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT t.workflow_id FROM issue_types t \
         WHERE t.id = ANY($1) AND t.deleted_at IS NULL AND t.workflow_id IS NOT NULL \
         GROUP BY t.workflow_id HAVING COUNT(*) > 1)",
    )
    .bind(&ids)
    .fetch_one(&st.pool)
    .await?;
    let (project_conflict,): (bool,) = sqlx::query_as(
        "SELECT EXISTS( \
           SELECT 1 FROM project_issue_types pit \
           JOIN issue_types t ON t.id = pit.issue_type_id \
           WHERE pit.project_id = $1 AND pit.deleted_at IS NULL AND t.deleted_at IS NULL \
             AND t.id <> ALL($2) AND t.workflow_id IS NOT NULL \
             AND t.workflow_id IN (SELECT workflow_id FROM issue_types WHERE id = ANY($2) AND workflow_id IS NOT NULL))",
    )
    .bind(project_id)
    .bind(&ids)
    .fetch_one(&st.pool)
    .await?;
    if batch_conflict || project_conflict {
        return Ok(bad(
            "Workflow is already enabled for another work item type in this project",
        ));
    }
    for id in &ids {
        sqlx::query(
            "INSERT INTO project_issue_types (id, issue_type_id, project_id, workspace_id, \
             level, is_default, created_by_id, updated_by_id, created_at, updated_at) \
             SELECT gen_random_uuid(), t.id, p.id, $3, 0, false, $4, $4, now(), now() \
             FROM issue_types t JOIN projects p ON p.id = $2 \
             WHERE t.id = $1 AND t.workspace_id = $3 AND t.deleted_at IS NULL \
             AND p.workspace_id = $3 AND p.deleted_at IS NULL \
             AND NOT EXISTS(SELECT 1 FROM project_issue_types pit \
               WHERE pit.project_id = p.id AND pit.issue_type_id = t.id AND pit.deleted_at IS NULL)",
        )
        .bind(id)
        .bind(project_id)
        .bind(ws)
        .bind(auth.0)
        .execute(&st.pool)
        .await?;
    }
    // Materialize hanya type yang benar-benar punya link hidup di project ini,
    // sehingga id asing/type dari workspace lain tidak ikut ter-materialize.
    let linked: Vec<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT pit.issue_type_id FROM project_issue_types pit \
         WHERE pit.project_id = $1 AND pit.deleted_at IS NULL AND pit.issue_type_id = ANY($2)",
    )
    .bind(project_id)
    .bind(&ids)
    .fetch_all(&st.pool)
    .await?;
    for (type_id,) in &linked {
        crate::routes::workflow::materialize_type_states(&st.pool, project_id, *type_id).await?;
    }
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}
