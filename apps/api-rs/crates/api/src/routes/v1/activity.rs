//! v1 work-item activities (`/api/v1/.../work-items/.../activities/...`).
//!
//! Full-column read path for the SDK `WorkItemActivity` shape: the app-API
//! handlers (`work_item::list_activities`/`get_activity`) return only
//! `{id, verb}`, but the SDK requires `project`/`workspace` (plus the
//! remaining activity columns), so this module queries `issue_activities`
//! directly. Gate mirrors `v1/work_item.rs::list_project` (ADMIN/MEMBER/GUEST
//! or workspace-ADMIN fallback via `project_gate_allows`).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};

use crate::routes::issue_common::{
    fetch_project_member_role, is_workspace_admin, project_gate_allows,
};
use crate::routes::project::{deny, missing};
use crate::routes::v1::common::{bad_request, page_rows, PageParams};
use crate::{middleware::auth::AuthUser, state::AppState};

type R = Result<(StatusCode, Json<Value>), common::errors::AppError>;

/// One `issue_activities` row for the v1 SDK shape. Field order matches
/// `ACTIVITY_COLS` so `query_as` binds positionally.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct V1ActivityRow {
    pub id: uuid::Uuid,
    pub verb: String,
    pub field: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub comment: Option<String>,
    pub attachments: Option<Vec<String>>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub old_identifier: Option<uuid::Uuid>,
    pub new_identifier: Option<uuid::Uuid>,
    pub epoch: Option<f64>,
    pub issue_id: Option<uuid::Uuid>,
    pub issue_comment_id: Option<uuid::Uuid>,
    pub actor_id: Option<uuid::Uuid>,
    pub project_id: uuid::Uuid,
    pub workspace_id: uuid::Uuid,
}

const ACTIVITY_COLS: &str = "id, verb, field, old_value, new_value, comment, attachments, \
    created_at, updated_at, deleted_at, old_identifier, new_identifier, epoch, \
    issue_id, issue_comment_id, actor_id, project_id, workspace_id";

/// SDK `WorkItemActivity` shape: model columns plus the `issue`/
/// `issue_comment`/`actor`/`project`/`workspace` aliases.
pub fn v1_activity_json(row: &V1ActivityRow) -> Value {
    json!({
        "id": row.id,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "deleted_at": row.deleted_at,
        "verb": row.verb,
        "field": row.field,
        "old_value": row.old_value,
        "new_value": row.new_value,
        "comment": row.comment,
        "attachments": row.attachments,
        "old_identifier": row.old_identifier,
        "new_identifier": row.new_identifier,
        "epoch": row.epoch,
        "issue": row.issue_id,
        "issue_comment": row.issue_comment_id,
        "actor": row.actor_id,
        "project": row.project_id,
        "workspace": row.workspace_id,
    })
}

/// Project-level read gate mirroring `v1/work_item.rs::list_project`:
/// ADMIN (20) / MEMBER (15) / GUEST (5), with the workspace-ADMIN fallback
/// from `project_gate_allows`.
async fn read_gate(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    slug: &str,
    project_id: uuid::Uuid,
) -> Result<bool, common::errors::AppError> {
    let member_role = fetch_project_member_role(pool, user_id, slug, project_id).await?;
    let ws_admin = is_workspace_admin(pool, user_id, slug).await?;
    Ok(project_gate_allows(
        matches!(member_role, Some(20) | Some(15) | Some(5)),
        member_role.is_some(),
        ws_admin,
    ))
}

pub async fn list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, issue_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    Query(q): Query<PageParams>,
) -> R {
    if !read_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let rows: Vec<V1ActivityRow> = sqlx::query_as(&format!(
        "SELECT {ACTIVITY_COLS} FROM issue_activities \
         WHERE project_id = $1 AND issue_id = $2 AND deleted_at IS NULL \
         ORDER BY created_at ASC"
    ))
    .bind(project_id)
    .bind(issue_id)
    .fetch_all(&st.pool)
    .await?;
    let values: Vec<Value> = rows.iter().map(v1_activity_json).collect();
    match page_rows(values, q.per_page.as_deref(), q.cursor.as_deref()) {
        Ok(v) => Ok((StatusCode::OK, Json(v))),
        Err(msg) => {
            let (s, v) = bad_request(msg);
            Ok((s, Json(v)))
        }
    }
}

pub async fn retrieve(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, issue_id, pk)): Path<(String, uuid::Uuid, uuid::Uuid, uuid::Uuid)>,
) -> R {
    if !read_gate(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let row: Option<V1ActivityRow> = sqlx::query_as(&format!(
        "SELECT {ACTIVITY_COLS} FROM issue_activities \
         WHERE id = $1 AND project_id = $2 AND issue_id = $3 AND deleted_at IS NULL"
    ))
    .bind(pk)
    .bind(project_id)
    .bind(issue_id)
    .fetch_optional(&st.pool)
    .await?;
    match row {
        Some(r) => Ok((StatusCode::OK, Json(v1_activity_json(&r)))),
        None => Ok(missing()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_row() -> V1ActivityRow {
        V1ActivityRow {
            id: uuid::Uuid::nil(),
            verb: "created".to_string(),
            field: Some("state".to_string()),
            old_value: Some("a".to_string()),
            new_value: Some("b".to_string()),
            comment: Some("note".to_string()),
            attachments: Some(vec!["http://x/y".to_string()]),
            created_at: None,
            updated_at: None,
            deleted_at: None,
            old_identifier: None,
            new_identifier: None,
            epoch: Some(1.5),
            issue_id: Some(uuid::Uuid::nil()),
            issue_comment_id: None,
            actor_id: None,
            project_id: uuid::Uuid::nil(),
            workspace_id: uuid::Uuid::nil(),
        }
    }

    #[test]
    fn activity_cols_covers_all_row_columns() {
        for col in [
            "id",
            "verb",
            "field",
            "old_value",
            "new_value",
            "comment",
            "attachments",
            "created_at",
            "updated_at",
            "deleted_at",
            "old_identifier",
            "new_identifier",
            "epoch",
            "issue_id",
            "issue_comment_id",
            "actor_id",
            "project_id",
            "workspace_id",
        ] {
            assert!(ACTIVITY_COLS.contains(col), "missing {col}");
        }
    }

    #[test]
    fn activity_json_aliases_fk_columns() {
        let v = v1_activity_json(&sample_row());
        assert_eq!(v["verb"], "created");
        assert_eq!(v["field"], "state");
        assert_eq!(v["old_value"], "a");
        assert_eq!(v["new_value"], "b");
        assert_eq!(v["comment"], "note");
        assert_eq!(v["epoch"], 1.5);
        // Alias keys carry the FK ids.
        assert!(v.get("issue").is_some());
        assert!(v.get("issue_comment").is_some());
        assert!(v.get("actor").is_some());
        assert!(v.get("project").is_some());
        assert!(v.get("workspace").is_some());
        assert_eq!(
            v["project"].as_str().unwrap(),
            uuid::Uuid::nil().to_string()
        );
        assert_eq!(
            v["workspace"].as_str().unwrap(),
            uuid::Uuid::nil().to_string()
        );
    }

    #[test]
    fn activity_json_nulls_when_options_none() {
        let mut row = sample_row();
        row.field = None;
        row.attachments = None;
        row.issue_id = None;
        let v = v1_activity_json(&row);
        assert!(v["field"].is_null());
        assert!(v["attachments"].is_null());
        assert!(v["issue"].is_null());
    }
}
