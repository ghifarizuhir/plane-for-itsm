//! v1 work-item-type data layer (`issue_types` + `project_issue_types`).
//! Handlers arrive in later tasks; this module holds only the row shape,
//! JSON shaper, create/update bodies, and auth helpers.

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::FromRow;

use crate::routes::project::{project_role, ws_role};

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
